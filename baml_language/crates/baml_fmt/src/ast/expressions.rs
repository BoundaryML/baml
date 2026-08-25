//! Reference: [`baml_db::baml_compiler_syntax::ast::Expr`] and [`baml_db::baml_compiler_hir::body`]

use baml_db::baml_compiler_syntax::{
    FromCST, SyntaxKind, ast as raw_ast,
    validated::{Validated, ValidatedElseBranch, ValidatedExprNode, ValidatedSyntaxToken},
};
use rowan::TextRange;

use crate::{
    ast::{Token, tokens as t},
    printer::{PrintInfo, Printable, Printer, Shape},
    trivia_classifier::{EmittableTrivia, TriviaInfo, TriviaSliceExt},
};

pub(crate) trait FunctionArrowLayout {
    fn arrow_span(&self) -> TextRange;
    fn print_separator_before(
        &self,
        next_leftmost: Option<TextRange>,
        continuation_indent: usize,
        printer: &mut Printer,
    );
}

impl FunctionArrowLayout for ValidatedSyntaxToken {
    fn arrow_span(&self) -> TextRange {
        self.span()
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

impl Printable for Validated<'_, raw_ast::ThrowsClause> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.throws_token());
        printer.print_str(" ");
        self.type_expr().print(shape, printer)
    }

    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for raw_ast::ThrowsClause {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let keyword = self.throws_token().expect("validated throws token");
        let ty = self.type_expr().expect("validated throws type");
        printer.print_input_range(keyword.text_range());
        printer.print_str(" ");
        ty.print(shape, printer)
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
            ValidatedExprNode::EnvAccessExpr(env) => {
                printer.print_raw_token(&env.env_token());
                printer.print_raw_token(&env.dot_token());
                printer.print_raw_token(&env.name_token());
                PrintInfo::default_single_line()
            }
            verbatim @ (ValidatedExprNode::TaggedTemplateExpr(_)
            | ValidatedExprNode::UpcastExpr(_)
            | ValidatedExprNode::QualifiedPathExpr(_)
            | ValidatedExprNode::SpecExpr(_)
            | ValidatedExprNode::ThrowExpr(_)
            | ValidatedExprNode::ReturnExpr(_)
            | ValidatedExprNode::BreakExpr(_)
            | ValidatedExprNode::ContinueExpr(_)
            | ValidatedExprNode::AwaitExpr(_)) => {
                let range = match verbatim {
                    ValidatedExprNode::TaggedTemplateExpr(node) => node.text_range(),
                    ValidatedExprNode::UpcastExpr(node) => node.text_range(),
                    ValidatedExprNode::QualifiedPathExpr(node) => node.text_range(),
                    ValidatedExprNode::SpecExpr(node) => node.text_range(),
                    ValidatedExprNode::ThrowExpr(node) => node.text_range(),
                    ValidatedExprNode::ReturnExpr(node) => node.text_range(),
                    ValidatedExprNode::BreakExpr(node) => node.text_range(),
                    ValidatedExprNode::ContinueExpr(node) => node.text_range(),
                    ValidatedExprNode::AwaitExpr(node) => node.text_range(),
                    _ => unreachable!("matched verbatim expression"),
                };
                printer.print_input_range_trimmed_start(range);
                PrintInfo {
                    multi_lined: printer.input[range].contains('\n'),
                }
            }
            ValidatedExprNode::BlockExpr(block) => block.print(shape, printer),
            ValidatedExprNode::PathExpr(_)
            | ValidatedExprNode::CallExpr(_)
            | ValidatedExprNode::IndexExpr(_)
            | ValidatedExprNode::FieldAccessExpr(_)
            | ValidatedExprNode::OptionalFieldAccessExpr(_)
            | ValidatedExprNode::OptionalIndexExpr(_)
            | ValidatedExprNode::OptionalCallExpr(_) => {
                ValidatedPrintChain::new(*self, printer.trivia).print(shape, printer)
            }
            ValidatedExprNode::MatchExpr(expression) => expression.print(shape, printer),
            ValidatedExprNode::CatchExpr(expression) => expression.print(shape, printer),
            ValidatedExprNode::SpawnExpr(expression) => expression.print(shape, printer),
            ValidatedExprNode::LambdaExpr(expression) => expression.print(shape, printer),
            ValidatedExprNode::ParenExpr(paren) => paren.print(shape, printer),
            ValidatedExprNode::UnaryExpr(unary) => unary.print(shape, printer),
            ValidatedExprNode::BinaryExpr(binary) => binary.print(shape, printer),
            ValidatedExprNode::IsExpr(is_expr) => is_expr.print(shape, printer),
            ValidatedExprNode::IfExpr(if_expr) => if_expr.print(shape, printer),
            ValidatedExprNode::IfLetExpr(if_expr) => if_expr.print(shape, printer),
            ValidatedExprNode::ArrayLiteral(array) => array.print(shape, printer),
            ValidatedExprNode::ObjectLiteral(object) => object.print(shape, printer),
            ValidatedExprNode::MapLiteral(map) => map.print(shape, printer),
            ValidatedExprNode::ForExpr(expression) => expression.print(shape, printer),
        }
    }

    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

#[derive(Clone, Copy)]
enum ValidatedObjectMember<'tree> {
    Field(Validated<'tree, raw_ast::ObjectField>),
    Spread(Validated<'tree, raw_ast::SpreadElement>),
}

impl Printable for ValidatedObjectMember<'_> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            Self::Field(field) => field.print(shape, printer),
            Self::Spread(spread) => spread.print(shape, printer),
        }
    }

    fn leftmost_token(&self) -> TextRange {
        match self {
            Self::Field(field) => field.leftmost_token(),
            Self::Spread(spread) => spread.leftmost_token(),
        }
    }

    fn rightmost_token(&self) -> TextRange {
        match self {
            Self::Field(field) => field.rightmost_token(),
            Self::Spread(spread) => spread.rightmost_token(),
        }
    }
}

impl Printable for Validated<'_, raw_ast::ObjectField> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut info = PrintInfo::default_single_line();
        if let Some(key) = self.key_token() {
            printer.print_raw_token(&key);
        } else if let Some(string) = self.string_literal() {
            info =
                t::QuotedString::new_from_span(string.text_range()).print(shape.clone(), printer);
        }
        if let Some((colon, value)) = self.colon_token().zip(self.value()) {
            printer.print_raw_token(&colon);
            printer.print_str(" ");
            printer.print_trivia_squished(printer.trivia.get_for_range_split(colon.span()).1);
            printer.print_trivia_squished(printer.trivia.get_leading_for_element(&value));
            info.multi_lined |= value.print(shape, printer).multi_lined;
        }
        info
    }

    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.value()
            .map_or_else(|| self.last_token_range(), |value| value.rightmost_token())
    }
}

impl Printable for Validated<'_, raw_ast::SpreadElement> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let dots = self.dot_dot_dot_token();
        let value = self.value();
        printer.print_raw_token(&dots);
        printer.print_trivia_squished(printer.trivia.get_for_range_split(dots.span()).1);
        printer.print_trivia_squished(printer.trivia.get_leading_for_element(&value));
        value.print(shape, printer)
    }

    fn leftmost_token(&self) -> TextRange {
        self.dot_dot_dot_token().span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.value().rightmost_token()
    }
}

fn validated_object_items(
    node: Validated<
        '_,
        impl rowan::ast::AstNode<Language = baml_db::baml_compiler_syntax::BamlLanguage>,
    >,
) -> Vec<(ValidatedObjectMember<'_>, Option<ValidatedSyntaxToken>)> {
    let mut items = Vec::new();
    for element in node.direct_elements() {
        let member = element
            .node::<raw_ast::ObjectField>()
            .map(ValidatedObjectMember::Field)
            .or_else(|| {
                element
                    .node::<raw_ast::SpreadElement>()
                    .map(ValidatedObjectMember::Spread)
            });
        if let Some(member) = member {
            items.push((member, None));
        } else if let Some(token) = element.token()
            && token.kind() == SyntaxKind::COMMA
            && let Some((_, comma)) = items.last_mut()
        {
            *comma = Some(token);
        }
    }
    items
}

fn print_validated_object_members_single_line(
    items: &[(ValidatedObjectMember<'_>, Option<ValidatedSyntaxToken>)],
    shape: &Shape,
    printer: &mut Printer,
) -> Option<()> {
    for (index, (member, comma)) in items.iter().enumerate() {
        if printer.output.len() > shape.width {
            return None;
        }
        let (leading, trailing) = printer.trivia.get_for_element(member);
        printer.try_print_trivia_single_line_squished(leading)?;
        if member
            .print(Shape::unlimited_single_line(), printer)
            .multi_lined
        {
            return None;
        }
        printer.try_print_trivia_single_line_squished(trailing)?;
        if index + 1 < items.len() {
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
            let (comma_leading, comma_trailing) = printer.trivia.get_for_range_split(comma.span());
            printer.try_print_trivia_single_line_squished(comma_leading)?;
            printer.try_print_trivia_single_line_squished(comma_trailing)?;
        }
    }
    Some(())
}

fn print_validated_object_members_multi_line(
    items: Vec<(ValidatedObjectMember<'_>, Option<ValidatedSyntaxToken>)>,
    inner_indent: usize,
    printer: &mut Printer,
) {
    let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
    for (member, comma) in items {
        printer.print_trivia_all_leading_with_newline_for(member.leftmost_token(), inner_indent);
        printer.print_spaces(inner_indent);
        member.print(inner_shape.clone(), printer);
        if let Some(comma) = comma {
            printer.print_raw_token(&comma);
            printer.print_trivia_all_trailing_for(comma.span());
        } else {
            printer.print_str(",");
            printer.print_trivia_all_trailing_for(member.rightmost_token());
        }
        printer.print_newline();
    }
}

impl Printable for Validated<'_, raw_ast::ObjectLiteral> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let items = validated_object_items(*self);
        printer
            .try_sub_printer(|p| {
                let constructor = self.constructor()?;
                constructor.print(Shape::unlimited_single_line(), p);
                p.print_str(" ");
                let open = self.l_brace_token();
                let close = self.r_brace_token();
                p.print_raw_token(&open);
                p.print_str(" ");
                p.try_print_trivia_single_line_squished(
                    p.trivia.get_for_range_split(open.span()).1,
                )?;
                print_validated_object_members_single_line(&items, &shape, p)?;
                p.try_print_trivia_single_line_squished(
                    p.trivia.get_for_range_split(close.span()).0,
                )?;
                p.print_str(" ");
                p.print_raw_token(&close);
                (p.output.len() <= shape.width).then(PrintInfo::default_single_line)
            })
            .unwrap_or_else(|| {
                let constructor = self.constructor().expect("validated object constructor");
                let open = self.l_brace_token();
                let close = self.r_brace_token();
                constructor.print(Shape::unlimited_single_line(), printer);
                printer.print_str(" ");
                printer.print_raw_token(&open);
                printer.print_trivia_all_trailing_for(open.span());
                printer.print_newline();
                print_validated_object_members_multi_line(
                    items,
                    shape.indent + printer.config.indent_width,
                    printer,
                );
                printer.print_spaces(shape.indent);
                printer.print_trivia_all_leading_with_newline_for(close.span(), shape.indent);
                printer.print_raw_token(&close);
                PrintInfo::default_multi_lined()
            })
    }

    fn leftmost_token(&self) -> TextRange {
        self.constructor()
            .map_or_else(|| self.l_brace_token().span(), |path| path.leftmost_token())
    }

    fn rightmost_token(&self) -> TextRange {
        self.r_brace_token().span()
    }
}

impl Printable for Validated<'_, raw_ast::MapLiteral> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let items = validated_object_items(*self);
        printer
            .try_sub_printer(|p| {
                let open = self.l_brace_token();
                let close = self.r_brace_token();
                let open_trailing = p.trivia.get_for_range_split(open.span()).1;
                let close_leading = p.trivia.get_for_range_split(close.span()).0;
                let has_content = !items.is_empty()
                    || open_trailing.iter().any(EmittableTrivia::is_comment)
                    || close_leading.iter().any(EmittableTrivia::is_comment);
                p.print_raw_token(&open);
                if has_content {
                    p.print_str(" ");
                }
                p.try_print_trivia_single_line_squished(open_trailing)?;
                print_validated_object_members_single_line(&items, &shape, p)?;
                p.try_print_trivia_single_line_squished(close_leading)?;
                if has_content {
                    p.print_str(" ");
                }
                p.print_raw_token(&close);
                (p.output.len() <= shape.width).then(PrintInfo::default_single_line)
            })
            .unwrap_or_else(|| {
                let open = self.l_brace_token();
                let close = self.r_brace_token();
                let inner_indent = shape.indent + printer.config.indent_width;
                printer.print_raw_token(&open);
                printer.print_trivia_all_trailing_for(open.span());
                printer.print_newline();
                print_validated_object_members_multi_line(items, inner_indent, printer);
                printer.print_trivia_all_leading_with_newline_for(close.span(), inner_indent);
                printer.print_spaces(shape.indent);
                printer.print_raw_token(&close);
                PrintInfo::default_multi_lined()
            })
    }

    fn leftmost_token(&self) -> TextRange {
        self.l_brace_token().span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.r_brace_token().span()
    }
}

#[derive(Clone, Copy)]
enum ValidatedMatchItem<'tree> {
    Arm(Validated<'tree, raw_ast::MatchArm>),
    Header(Validated<'tree, raw_ast::HeaderComment>),
}

impl Printable for ValidatedMatchItem<'_> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            Self::Arm(arm) => arm.print(shape, printer),
            Self::Header(header) => {
                printer.print_input_range_trimmed_start(header.text_range());
                PrintInfo::default_single_line()
            }
        }
    }

    fn leftmost_token(&self) -> TextRange {
        match self {
            Self::Arm(arm) => arm.leftmost_token(),
            Self::Header(header) => header.first_token_range(),
        }
    }

    fn rightmost_token(&self) -> TextRange {
        match self {
            Self::Arm(arm) => arm.rightmost_token(),
            Self::Header(header) => header.last_token_range(),
        }
    }
}

fn validated_match_items(
    expression: Validated<'_, raw_ast::MatchExpr>,
) -> Vec<ValidatedMatchItem<'_>> {
    expression
        .direct_elements()
        .filter_map(|element| {
            element
                .node::<raw_ast::MatchArm>()
                .map(ValidatedMatchItem::Arm)
                .or_else(|| {
                    element
                        .node::<raw_ast::HeaderComment>()
                        .map(ValidatedMatchItem::Header)
                })
        })
        .collect()
}

fn print_validated_match_scrutinee_single_line(
    expression: Validated<'_, raw_ast::MatchExpr>,
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    let scrutinee = expression.scrutinee()?;
    let Some(open) = expression.l_paren_token() else {
        let (leading, trailing) = printer.trivia.get_for_element(&scrutinee);
        printer.try_print_trivia_single_line_squished(leading)?;
        if scrutinee
            .print(Shape::unlimited_single_line(), printer)
            .multi_lined
        {
            return None;
        }
        printer.try_print_trivia_single_line_squished(trailing)?;
        return (printer.output.len() <= shape.width).then(PrintInfo::default_single_line);
    };
    let close = expression.r_paren_token()?;
    printer.print_raw_token(&open);
    printer
        .try_print_trivia_single_line_squished(printer.trivia.get_for_range_split(open.span()).1)?;
    let (leading, trailing) = printer.trivia.get_for_element(&scrutinee);
    printer.try_print_trivia_single_line_squished(leading)?;
    if scrutinee
        .print(Shape::unlimited_single_line(), printer)
        .multi_lined
    {
        return None;
    }
    printer.try_print_trivia_single_line_squished(trailing)?;
    if let Some((colon, ty)) = expression.colon_token().zip(expression.type_expr()) {
        printer.print_raw_token(&colon);
        printer.print_str(" ");
        if ty
            .print(Shape::unlimited_single_line(), printer)
            .multi_lined
        {
            return None;
        }
    }
    printer.try_print_trivia_single_line_squished(
        printer.trivia.get_for_range_split(close.span()).0,
    )?;
    printer.print_raw_token(&close);
    (printer.output.len() <= shape.width).then(PrintInfo::default_single_line)
}

impl Printable for Validated<'_, raw_ast::MatchExpr> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let keyword = self.match_token();
        printer.print_raw_token(&keyword);
        printer.print_str(" ");
        if printer
            .try_sub_printer(|probe| {
                print_validated_match_scrutinee_single_line(*self, &shape, probe)
            })
            .is_none()
        {
            let scrutinee = self.scrutinee().expect("validated match scrutinee");
            if let Some(open) = self.l_paren_token() {
                let close = self.r_paren_token().expect("validated match close paren");
                let inner_indent = shape.indent + printer.config.indent_width;
                printer.print_raw_token(&open);
                printer.print_newline();
                printer.print_standalone_with_trivia(&scrutinee, inner_indent);
                if let Some((colon, ty)) = self.colon_token().zip(self.type_expr()) {
                    printer.print_raw_token(&colon);
                    printer.print_str(" ");
                    ty.print(
                        Shape::standalone(printer.config.line_width, inner_indent),
                        printer,
                    );
                }
                printer.print_newline();
                printer.print_spaces(shape.indent);
                printer.print_raw_token(&close);
            } else {
                scrutinee.print(shape.clone(), printer);
            }
        }
        printer.print_str(" ");
        let open = self.l_brace_token();
        let close = self.r_brace_token();
        let inner_indent = shape.indent + printer.config.indent_width;
        printer.print_raw_token(&open);
        printer.print_trivia_all_trailing_for(open.span());
        printer.print_newline();
        for item in validated_match_items(*self) {
            printer.print_standalone_with_trivia(&item, inner_indent);
            printer.print_newline();
        }
        printer.print_trivia_all_leading_with_newline_for(close.span(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&close);
        PrintInfo::default_multi_lined()
    }

    fn leftmost_token(&self) -> TextRange {
        self.match_token().span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.r_brace_token().span()
    }
}

impl Printable for Validated<'_, raw_ast::MatchGuard> {
    fn print(&self, mut shape: Shape, printer: &mut Printer) -> PrintInfo {
        let keyword = self.if_token();
        printer.print_raw_token(&keyword);
        printer.print_str(" ");
        let width = usize::from(keyword.span().len()) + 1;
        shape.width = shape.width.saturating_sub(width);
        shape.first_line_offset += width;
        self.condition().print(shape, printer)
    }

    fn leftmost_token(&self) -> TextRange {
        self.if_token().span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.condition().rightmost_token()
    }
}

fn print_validated_match_condition(
    arm: Validated<'_, raw_ast::MatchArm>,
    shape: &Shape,
    printer: &mut Printer,
) -> PrintInfo {
    let pattern = arm.pattern();
    let pattern_info = pattern.print(shape.clone(), printer);
    let multi_lined = pattern_info.multi_lined;
    if let Some(guard) = arm.match_guard() {
        let mut probe = printer.sub_printer();
        let guard_info = guard.print(Shape::unlimited_single_line(), &mut probe);
        if pattern_info.multi_lined
            || guard_info.multi_lined
            || printer.current_line_len() + 1 + probe.output.len() + 3 > printer.config.line_width
        {
            printer.print_newline();
            printer.print_spaces(shape.indent + printer.config.indent_width);
            guard.print(
                Shape::standalone(
                    printer.config.line_width,
                    shape.indent + printer.config.indent_width,
                ),
                printer,
            );
        } else {
            printer.print_str(" ");
            printer.append_from_printer(probe);
        }
    }
    printer.print_str(" =>");
    PrintInfo { multi_lined }
}

fn validated_jump_expression(expression: Validated<'_, raw_ast::ExprNode>) -> bool {
    matches!(
        expression.as_variant(),
        ValidatedExprNode::ReturnExpr(_)
            | ValidatedExprNode::BreakExpr(_)
            | ValidatedExprNode::ContinueExpr(_)
    )
}

fn print_validated_wrapped_arm_body(
    printer: &mut Printer,
    body: Validated<'_, raw_ast::ExprNode>,
    arm_indent: usize,
) {
    let inner_indent = arm_indent + printer.config.indent_width;
    printer.print_standalone_leading_and_body(&body, inner_indent);
    if validated_jump_expression(body) {
        printer.print_str(";");
    } else {
        printer.print_trivia_trailing(printer.trivia.get_trailing_for_element(&body));
    }
}

impl Printable for Validated<'_, raw_ast::MatchArm> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let condition = print_validated_match_condition(*self, &shape, printer);
        let body = self.value();
        if condition.multi_lined {
            printer.print_newline();
            printer.print_spaces(shape.indent);
            if let ValidatedExprNode::BlockExpr(block) = body.as_variant() {
                block.print(shape.clone(), printer);
                printer.print_str(",");
            } else {
                printer.print_str("{");
                printer.print_newline();
                print_validated_wrapped_arm_body(printer, body, shape.indent);
                printer.print_newline();
                printer.print_spaces(shape.indent);
                printer.print_str("},");
            }
            return PrintInfo::default_multi_lined();
        }
        printer.print_str(" ");
        let remaining = printer.current_line_remaining_width();
        if let ValidatedExprNode::BlockExpr(block) = body.as_variant() {
            let info = block.print(
                Shape {
                    width: remaining,
                    indent: shape.indent,
                    first_line_offset: printer
                        .config
                        .line_width
                        .saturating_sub(shape.indent + remaining),
                },
                printer,
            );
            printer.print_str(",");
            return info;
        }
        if let ValidatedExprNode::MatchExpr(expression) = body.as_variant() {
            let mut probe = printer.sub_printer();
            probe.print_raw_token(&expression.match_token());
            probe.print_str(" ");
            let header_fits = print_validated_match_scrutinee_single_line(
                expression,
                &Shape::unlimited_single_line(),
                &mut probe,
            )
            .is_some_and(|_| probe.output.len() + const { " {".len() } <= remaining);
            if header_fits {
                let info = expression.print(
                    Shape {
                        width: remaining,
                        indent: shape.indent,
                        first_line_offset: printer
                            .config
                            .line_width
                            .saturating_sub(shape.indent + remaining),
                    },
                    printer,
                );
                printer.print_str(",");
                return info;
            }
        }
        let mut probe = printer.sub_printer();
        let body_info = body.print(Shape::unlimited_single_line(), &mut probe);
        if validated_jump_expression(body)
            || body_info.multi_lined
            || probe.output.len() > remaining
        {
            printer.print_str("{");
            printer.print_newline();
            print_validated_wrapped_arm_body(printer, body, shape.indent);
            printer.print_newline();
            printer.print_spaces(shape.indent);
            printer.print_str("},");
            PrintInfo::default_multi_lined()
        } else {
            printer.append_from_printer(probe);
            printer.print_str(",");
            PrintInfo::default_single_line()
        }
    }

    fn leftmost_token(&self) -> TextRange {
        self.pattern().leftmost_token()
    }

    fn rightmost_token(&self) -> TextRange {
        self.comma_token()
            .map_or_else(|| self.value().rightmost_token(), |comma| comma.span())
    }
}

#[derive(Clone, Copy)]
enum ValidatedCatchItem<'tree> {
    Arm(Validated<'tree, raw_ast::CatchArm>),
    Header(Validated<'tree, raw_ast::HeaderComment>),
}

impl Printable for ValidatedCatchItem<'_> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            Self::Arm(arm) => arm.print(shape, printer),
            Self::Header(header) => {
                printer.print_input_range_trimmed_start(header.text_range());
                PrintInfo::default_single_line()
            }
        }
    }

    fn leftmost_token(&self) -> TextRange {
        match self {
            Self::Arm(arm) => arm.leftmost_token(),
            Self::Header(header) => header.first_token_range(),
        }
    }

    fn rightmost_token(&self) -> TextRange {
        match self {
            Self::Arm(arm) => arm.rightmost_token(),
            Self::Header(header) => header.last_token_range(),
        }
    }
}

fn validated_catch_items(
    clause: Validated<'_, raw_ast::CatchClause>,
) -> Vec<ValidatedCatchItem<'_>> {
    clause
        .direct_elements()
        .filter_map(|element| {
            element
                .node::<raw_ast::CatchArm>()
                .map(ValidatedCatchItem::Arm)
                .or_else(|| {
                    element
                        .node::<raw_ast::HeaderComment>()
                        .map(ValidatedCatchItem::Header)
                })
        })
        .collect()
}

impl Printable for Validated<'_, raw_ast::CatchExpr> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let body = self.body();
        let mut info = body.print(shape.clone(), printer);
        for clause in self.catch_clause() {
            printer.print_str(" ");
            info.multi_lined |= clause.print(shape.clone(), printer).multi_lined;
        }
        info
    }

    fn leftmost_token(&self) -> TextRange {
        self.body().leftmost_token()
    }

    fn rightmost_token(&self) -> TextRange {
        self.catch_clause()
            .last()
            .expect("validated catch clause")
            .rightmost_token()
    }
}

impl Printable for Validated<'_, raw_ast::CatchClause> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let keyword = self
            .catch_token()
            .or(self.catch_all_token())
            .or(self.catch_all_panics_token())
            .expect("validated catch keyword");
        let open_paren = self.l_paren_token();
        let close_paren = self.r_paren_token();
        let open_brace = self.l_brace_token();
        let close_brace = self.r_brace_token();
        printer.print_raw_token(&keyword);
        printer.print_str(" ");
        printer.print_raw_token(&open_paren);
        printer.print_raw_token(&self.catch_binding().name_token());
        if let Some(binding) = self.catch_stack_trace_binding() {
            printer.print_str(", ");
            printer.print_raw_token(&binding.name_token());
        }
        printer.print_raw_token(&close_paren);
        printer.print_str(" ");
        printer.print_raw_token(&open_brace);
        printer.print_trivia_all_trailing_for(open_brace.span());
        printer.print_newline();
        let inner_indent = shape.indent + printer.config.indent_width;
        for item in validated_catch_items(*self) {
            printer.print_standalone_with_trivia(&item, inner_indent);
            printer.print_newline();
        }
        printer.print_trivia_all_leading_with_newline_for(close_brace.span(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&close_brace);
        PrintInfo::default_multi_lined()
    }

    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.r_brace_token().span()
    }
}

impl Printable for Validated<'_, raw_ast::CatchArm> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let pattern = self.pattern();
        pattern.print(shape.clone(), printer);
        if let Some(binding) = self.catch_stack_trace_binding() {
            printer.print_str(", ");
            printer.print_raw_token(&binding.name_token());
        }
        printer.print_str(" ");
        printer.print_raw_token(&self.fat_arrow_token());
        printer.print_str(" ");
        let body = self.value();
        let remaining = printer.current_line_remaining_width();
        if let ValidatedExprNode::BlockExpr(block) = body.as_variant() {
            let info = block.print(
                Shape {
                    width: remaining,
                    indent: shape.indent,
                    first_line_offset: printer
                        .config
                        .line_width
                        .saturating_sub(shape.indent + remaining),
                },
                printer,
            );
            if self.comma_token().is_some() {
                printer.print_str(",");
            }
            return info;
        }
        let mut probe = printer.sub_printer();
        let body_info = body.print(Shape::unlimited_single_line(), &mut probe);
        if validated_jump_expression(body)
            || body_info.multi_lined
            || probe.output.len() > remaining
        {
            printer.print_str("{");
            printer.print_newline();
            print_validated_wrapped_arm_body(printer, body, shape.indent);
            printer.print_newline();
            printer.print_spaces(shape.indent);
            printer.print_str("}");
            if self.comma_token().is_some() {
                printer.print_str(",");
            }
            PrintInfo::default_multi_lined()
        } else {
            printer.append_from_printer(probe);
            if self.comma_token().is_some() {
                printer.print_str(",");
            }
            PrintInfo::default_single_line()
        }
    }

    fn leftmost_token(&self) -> TextRange {
        self.pattern().leftmost_token()
    }

    fn rightmost_token(&self) -> TextRange {
        self.comma_token()
            .map_or_else(|| self.value().rightmost_token(), |comma| comma.span())
    }
}

impl Printable for Validated<'_, raw_ast::LambdaExpr> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        if let Some(generics) = self.generic_param_list() {
            generics.print(shape.clone(), printer);
        }
        self.parameter_list().print(shape.clone(), printer);
        printer.print_str(" ->");
        let arrow = self
            .arrow_token()
            .or(self.fat_arrow_token())
            .expect("validated lambda arrow");
        if let Some(return_type) = self.type_expr() {
            arrow.print_separator_before(
                Some(return_type.leftmost_token()),
                shape.indent + printer.config.indent_width,
                printer,
            );
            return_type.print(shape.clone(), printer);
            if let Some(throws) = self.throws_clause() {
                printer.print_str(" ");
                throws.print(shape.clone(), printer);
            }
            printer.print_str(" ");
        } else if let Some(throws) = self.throws_clause() {
            arrow.print_separator_before(
                Some(throws.leftmost_token()),
                shape.indent + printer.config.indent_width,
                printer,
            );
            throws.print(shape.clone(), printer);
            printer.print_str(" ");
        } else {
            arrow.print_separator_before(
                Some(self.body().leftmost_token()),
                shape.indent + printer.config.indent_width,
                printer,
            );
        }
        self.body().print(shape, printer);
        PrintInfo::default_multi_lined()
    }

    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.body().rightmost_token()
    }
}

impl Printable for Validated<'_, raw_ast::GenericParamList> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.less_token());
        let params = self.generic_param().collect::<Vec<_>>();
        for (index, parameter) in params.iter().enumerate() {
            parameter.print(shape.clone(), printer);
            if index + 1 < params.len() {
                printer.print_str(", ");
            }
        }
        printer.print_raw_token(&self.greater_token());
        PrintInfo::default_single_line()
    }

    fn leftmost_token(&self) -> TextRange {
        self.less_token().span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.greater_token().span()
    }
}

impl Printable for Validated<'_, raw_ast::GenericParam> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.name_token());
        if let Some(bounds) = self.generic_param_bounds() {
            printer.print_str(" ");
            bounds.print(shape, printer)
        } else {
            PrintInfo::default_single_line()
        }
    }

    fn leftmost_token(&self) -> TextRange {
        self.name_token().span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.generic_param_bounds().map_or_else(
            || self.name_token().span(),
            |bounds| bounds.rightmost_token(),
        )
    }
}

impl Printable for Validated<'_, raw_ast::GenericParamBounds> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.extends_token());
        for (index, bound) in self.type_expr().enumerate() {
            printer.print_str(if index == 0 { " " } else { " & " });
            bound.print(shape.clone(), printer);
        }
        PrintInfo::default_single_line()
    }

    fn leftmost_token(&self) -> TextRange {
        self.extends_token().span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.type_expr().last().map_or_else(
            || self.extends_token().span(),
            |bound| bound.rightmost_token(),
        )
    }
}

fn validated_spawn_tail(
    block: Validated<'_, raw_ast::BlockExpr>,
) -> Option<Validated<'_, raw_ast::ExprNode>> {
    let mut items = block
        .direct_elements()
        .filter_map(|element| element.node::<raw_ast::BlockItem>());
    let item = items.next()?;
    if items.next().is_some() {
        return None;
    }
    item.cast::<raw_ast::ExprNode>()
}

fn validated_spawn_header_has_comments(
    spawn: Validated<'_, raw_ast::SpawnExpr>,
    printer: &Printer<'_>,
) -> bool {
    let start = spawn.spawn_token().span().start();
    let end = spawn.body().l_brace_token().span().start();
    printer.trivia.all_trivia().iter().any(|trivia| {
        let at = trivia.attached_to().start();
        trivia.is_comment() && at >= start && at <= end
    })
}

fn print_validated_spawn_header(
    spawn: Validated<'_, raw_ast::SpawnExpr>,
    shape: &Shape,
    printer: &mut Printer,
) -> PrintInfo {
    printer.print_raw_token(&spawn.spawn_token());
    let mut info = PrintInfo::default_single_line();
    if let Some(name) = spawn.name() {
        printer.print_str(" ");
        info.multi_lined |= name.print(shape.clone(), printer).multi_lined;
    }
    if let Some(with_token) = spawn.with_token() {
        printer.print_str(" ");
        printer.print_raw_token(&with_token);
        for (index, option) in spawn.expr_node().enumerate() {
            printer.print_str(if index == 0 { " " } else { ", " });
            info.multi_lined |= option.print(shape.clone(), printer).multi_lined;
        }
    }
    info
}

fn print_validated_spawn_single_line(
    spawn: Validated<'_, raw_ast::SpawnExpr>,
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    if validated_spawn_header_has_comments(spawn, printer) {
        return None;
    }
    let block = spawn.body();
    let items = block
        .direct_elements()
        .filter(|element| element.node::<raw_ast::BlockItem>().is_some())
        .count();
    if items > 1 || block.semicolon_tokens().next().is_some() {
        return None;
    }
    if print_validated_spawn_header(spawn, &Shape::unlimited_single_line(), printer).multi_lined {
        return None;
    }
    printer.print_str(" ");
    let open = block.l_brace_token();
    let close = block.r_brace_token();
    let open_trailing = printer.trivia.get_for_range_split(open.span()).1;
    let close_leading = printer.trivia.get_for_range_split(close.span()).0;
    printer.print_raw_token(&open);
    if let Some(tail) = validated_spawn_tail(block) {
        printer.print_str(" ");
        printer.try_print_trivia_single_line_squished(open_trailing)?;
        let (leading, trailing) = printer.trivia.get_for_element(&tail);
        printer.try_print_trivia_single_line_squished(leading)?;
        if tail
            .print(Shape::unlimited_single_line(), printer)
            .multi_lined
        {
            return None;
        }
        printer.try_print_trivia_single_line_squished(trailing)?;
        printer.try_print_trivia_single_line_squished(close_leading)?;
        printer.print_str(" ");
    } else if open_trailing.iter().any(EmittableTrivia::is_comment)
        || close_leading.iter().any(EmittableTrivia::is_comment)
    {
        return None;
    }
    printer.print_raw_token(&close);
    (printer.output.len() <= shape.width).then(PrintInfo::default_single_line)
}

impl Printable for Validated<'_, raw_ast::SpawnExpr> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|probe| print_validated_spawn_single_line(*self, &shape, probe))
            .unwrap_or_else(|| {
                if validated_spawn_header_has_comments(*self, printer) {
                    printer.print_input_range(TextRange::new(
                        self.spawn_token().span().start(),
                        self.body().l_brace_token().span().start(),
                    ));
                } else {
                    print_validated_spawn_header(*self, &shape, printer);
                    printer.print_str(" ");
                }
                self.body().print(shape, printer);
                PrintInfo::default_multi_lined()
            })
    }

    fn leftmost_token(&self) -> TextRange {
        self.spawn_token().span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.body().rightmost_token()
    }
}

fn validated_array_items(
    array: Validated<'_, raw_ast::ArrayLiteral>,
) -> Vec<(
    Validated<'_, raw_ast::ExprNode>,
    Option<ValidatedSyntaxToken>,
)> {
    let mut items = Vec::new();
    for element in array.direct_elements() {
        if let Some(expr) = element.node::<raw_ast::ExprNode>() {
            items.push((expr, None));
        } else if let Some(token) = element.token()
            && token.kind() == SyntaxKind::COMMA
            && let Some((_, comma)) = items.last_mut()
        {
            *comma = Some(token);
        }
    }
    items
}

fn print_validated_array_single_line(
    array: Validated<'_, raw_ast::ArrayLiteral>,
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    let open = array.l_bracket_token();
    let close = array.r_bracket_token();
    let items = validated_array_items(array);
    printer.print_raw_token(&open);
    printer
        .try_print_trivia_single_line_squished(printer.trivia.get_for_range_split(open.span()).1)?;
    for (index, (expr, comma)) in items.iter().enumerate() {
        let (leading, trailing) = printer.trivia.get_for_element(expr);
        printer.try_print_trivia_single_line_squished(leading)?;
        if expr
            .print(Shape::unlimited_single_line(), printer)
            .multi_lined
        {
            return None;
        }
        printer.try_print_trivia_single_line_squished(trailing)?;
        if index + 1 < items.len() {
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
            let (comma_leading, comma_trailing) = printer.trivia.get_for_range_split(comma.span());
            printer.try_print_trivia_single_line_squished(comma_leading)?;
            printer.try_print_trivia_single_line_squished(comma_trailing)?;
        }
    }
    printer.try_print_trivia_single_line_squished(
        printer.trivia.get_for_range_split(close.span()).0,
    )?;
    printer.print_raw_token(&close);
    (printer.output.len() <= shape.width).then(PrintInfo::default_single_line)
}

fn print_validated_array_multi_line(
    array: Validated<'_, raw_ast::ArrayLiteral>,
    shape: &Shape,
    printer: &mut Printer,
) -> PrintInfo {
    let open = array.l_bracket_token();
    let close = array.r_bracket_token();
    let inner_indent = shape.indent + printer.config.indent_width;
    printer.print_raw_token(&open);
    printer.print_trivia_all_trailing_for(open.span());
    printer.print_newline();
    for (expr, comma) in validated_array_items(array) {
        let (leading, trailing) = printer.trivia.get_for_element(&expr);
        printer.print_trivia_with_newline(leading.trim_blanks(), inner_indent);
        printer.print_spaces(inner_indent);
        expr.print(
            Shape::standalone(printer.config.line_width, inner_indent),
            printer,
        );
        if let Some(comma) = comma {
            printer.print_trivia_squished(trailing);
            printer.print_raw_token(&comma);
            printer.print_trivia_all_trailing_for(comma.span());
        } else {
            printer.print_str(",");
            printer.print_trivia_trailing(trailing);
        }
        printer.print_newline();
    }
    printer.print_trivia_with_newline(
        printer
            .trivia
            .get_for_range_split(close.span())
            .0
            .trim_trailing_blanks(),
        inner_indent,
    );
    printer.print_spaces(shape.indent);
    printer.print_raw_token(&close);
    PrintInfo::default_multi_lined()
}

impl Printable for Validated<'_, raw_ast::ArrayLiteral> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| print_validated_array_single_line(*self, &shape, p))
            .unwrap_or_else(|| print_validated_array_multi_line(*self, &shape, printer))
    }

    fn leftmost_token(&self) -> TextRange {
        self.l_bracket_token().span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.r_bracket_token().span()
    }
}

impl Printable for Validated<'_, raw_ast::ElseBranch> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self.as_variant() {
            ValidatedElseBranch::BlockExpr(block) => block.print(shape, printer),
            ValidatedElseBranch::IfExpr(if_expr) => if_expr.print(shape, printer),
            ValidatedElseBranch::IfLetExpr(if_expr) => if_expr.print(shape, printer),
        }
    }

    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for Validated<'_, raw_ast::IfExpr> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let condition = self.condition();
        printer.print_raw_token(&self.if_token());
        printer.print_str(" ");
        let needs_parens = !matches!(condition.as_variant(), ValidatedExprNode::ParenExpr(_));
        let condition_shape = if needs_parens {
            Shape {
                width: shape.width.saturating_sub(2),
                ..shape
            }
        } else {
            shape.clone()
        };
        if needs_parens {
            printer.print_str("(");
        }
        condition.print(condition_shape, printer);
        if needs_parens {
            printer.print_str(")");
        }
        printer.print_str(" ");
        self.then_branch().print(shape.clone(), printer);
        if let Some((keyword, branch)) = self.else_token().zip(self.else_branch()) {
            printer.print_str(" ");
            printer.print_raw_token(&keyword);
            printer.print_str(" ");
            branch.print(shape, printer);
        }
        PrintInfo::default_multi_lined()
    }

    fn leftmost_token(&self) -> TextRange {
        self.if_token().span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.else_branch().map_or_else(
            || self.then_branch().rightmost_token(),
            |branch| branch.rightmost_token(),
        )
    }
}

impl Printable for Validated<'_, raw_ast::IfLetExpr> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.if_token());
        printer.print_str(" ");
        self.pattern().print(shape.clone(), printer);
        printer.print_str(" ");
        printer.print_raw_token(&self.equals_token());
        printer.print_str(" ");
        self.scrutinee().print(shape.clone(), printer);
        printer.print_str(" ");
        self.then_branch().print(shape.clone(), printer);
        if let Some((keyword, branch)) = self.else_token().zip(self.else_branch()) {
            printer.print_str(" ");
            printer.print_raw_token(&keyword);
            printer.print_str(" ");
            branch.print(shape, printer);
        }
        PrintInfo::default_multi_lined()
    }

    fn leftmost_token(&self) -> TextRange {
        self.if_token().span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.else_branch().map_or_else(
            || self.then_branch().rightmost_token(),
            |branch| branch.rightmost_token(),
        )
    }
}

impl Printable for Validated<'_, raw_ast::IsExpr> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let lhs = self.lhs();
        let keyword = self.is_token();
        let pattern = self.pattern();
        let mut info = lhs.print(shape.clone(), printer);
        let mut left_trivia =
            printer.print_trivia_squished(printer.trivia.get_trailing_for_element(&lhs));
        let (keyword_leading, keyword_trailing) =
            printer.trivia.get_for_range_split(keyword.span());
        left_trivia += printer.print_trivia_squished(keyword_leading);
        if left_trivia == 0 {
            printer.print_spaces(1);
        }
        printer.print_raw_token(&keyword);
        let mut right_trivia = printer.print_trivia_squished(keyword_trailing);
        right_trivia +=
            printer.print_trivia_squished(printer.trivia.get_leading_for_element(&pattern));
        if right_trivia == 0 {
            printer.print_spaces(1);
        }
        info.multi_lined |= pattern.print(shape, printer).multi_lined;
        info
    }

    fn leftmost_token(&self) -> TextRange {
        self.lhs().leftmost_token()
    }

    fn rightmost_token(&self) -> TextRange {
        self.pattern().rightmost_token()
    }
}

#[derive(Clone, Copy)]
struct ValidatedBinaryOperator {
    kind: SyntaxKind,
    range: TextRange,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BinaryOpChainingGroup {
    AddSubtract,
    MultiplyDivide,
    Bitwise,
    Logical,
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

impl Token for ValidatedBinaryOperator {
    fn span(&self) -> TextRange {
        self.range
    }
}

impl Printable for ValidatedBinaryOperator {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(self);
        PrintInfo::default_single_line()
    }

    fn leftmost_token(&self) -> TextRange {
        self.range
    }

    fn rightmost_token(&self) -> TextRange {
        self.range
    }
}

fn validated_binary_operator(
    binary: Validated<'_, raw_ast::BinaryExpr>,
) -> ValidatedBinaryOperator {
    let mut tokens = binary
        .direct_elements()
        .filter_map(|element| element.token());
    let first = tokens.next().expect("validated binary operator");
    let range = if first.kind() == SyntaxKind::QUESTION {
        let second = tokens.next().expect("validated null-coalescing operator");
        TextRange::new(first.text_range().start(), second.text_range().end())
    } else {
        first.text_range()
    };
    ValidatedBinaryOperator {
        kind: first.kind(),
        range,
    }
}

fn validated_binary_group(operator: ValidatedBinaryOperator) -> Option<BinaryOpChainingGroup> {
    match operator.kind {
        SyntaxKind::PLUS | SyntaxKind::MINUS => Some(BinaryOpChainingGroup::AddSubtract),
        SyntaxKind::STAR | SyntaxKind::SLASH | SyntaxKind::PERCENT => {
            Some(BinaryOpChainingGroup::MultiplyDivide)
        }
        SyntaxKind::AND | SyntaxKind::PIPE | SyntaxKind::CARET => {
            Some(BinaryOpChainingGroup::Bitwise)
        }
        SyntaxKind::AND_AND | SyntaxKind::OR_OR => Some(BinaryOpChainingGroup::Logical),
        _ => None,
    }
}

fn validated_binary_row(operator: ValidatedBinaryOperator) -> Option<BinaryOpPrecedenceRow> {
    match operator.kind {
        SyntaxKind::PLUS | SyntaxKind::MINUS => Some(BinaryOpPrecedenceRow::AddSubtract),
        SyntaxKind::STAR | SyntaxKind::SLASH | SyntaxKind::PERCENT => {
            Some(BinaryOpPrecedenceRow::MultiplyDivideModulo)
        }
        SyntaxKind::AND => Some(BinaryOpPrecedenceRow::BitwiseAnd),
        SyntaxKind::PIPE => Some(BinaryOpPrecedenceRow::BitwiseOr),
        SyntaxKind::CARET => Some(BinaryOpPrecedenceRow::BitwiseXor),
        SyntaxKind::AND_AND => Some(BinaryOpPrecedenceRow::LogicalAnd),
        SyntaxKind::OR_OR => Some(BinaryOpPrecedenceRow::LogicalOr),
        _ => None,
    }
}

fn validated_peel_transparent_parens<'tree>(
    mut expr: Validated<'tree, raw_ast::ExprNode>,
    trivia: &TriviaInfo,
) -> Validated<'tree, raw_ast::ExprNode> {
    loop {
        let ValidatedExprNode::ParenExpr(paren) = expr.as_variant() else {
            return expr;
        };
        if !validated_paren_is_transparent(paren, trivia) {
            return expr;
        }
        expr = paren.expr_node();
    }
}

fn validated_call_arg_expr<'tree>(
    argument: Validated<'tree, raw_ast::CallArg>,
    trivia: &TriviaInfo,
) -> Validated<'tree, raw_ast::ExprNode> {
    let expr = argument.value();
    let peeled = validated_peel_transparent_parens(expr, trivia);
    if matches!(
        peeled.as_variant(),
        ValidatedExprNode::LambdaExpr(_) | ValidatedExprNode::SpawnExpr(_)
    ) {
        expr
    } else {
        peeled
    }
}

fn validated_call_arg_is_huggable(argument: Validated<'_, raw_ast::CallArg>) -> bool {
    matches!(
        argument.value().as_variant(),
        ValidatedExprNode::LambdaExpr(_) | ValidatedExprNode::SpawnExpr(_)
    )
}

impl Printable for Validated<'_, raw_ast::CallArg> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let expr = validated_call_arg_expr(*self, printer.trivia);
        if let Some(name) = self.name_token() {
            let equals = self.equals_token().expect("validated named argument");
            printer.print_raw_token(&name);
            let (_, name_trailing) = printer.trivia.get_for_range_split(name.span());
            let (equals_leading, equals_trailing) =
                printer.trivia.get_for_range_split(equals.span());
            printer.print_trivia_squished(name_trailing);
            printer.print_trivia_squished(equals_leading);
            printer.print_str(" = ");
            printer.print_trivia_squished(equals_trailing);
            printer.print_trivia_squished(printer.trivia.get_leading_for_element(&expr));
        }
        expr.print(shape, printer)
    }

    fn leftmost_token(&self) -> TextRange {
        self.name_token()
            .map_or_else(|| self.value().leftmost_token(), |token| token.span())
    }

    fn rightmost_token(&self) -> TextRange {
        self.value().rightmost_token()
    }
}

fn validated_call_args_items(
    args: Validated<'_, raw_ast::CallArgs>,
) -> Vec<(
    Validated<'_, raw_ast::CallArg>,
    Option<ValidatedSyntaxToken>,
)> {
    let mut items = Vec::new();
    for element in args.direct_elements() {
        if let Some(argument) = element.node::<raw_ast::CallArg>() {
            items.push((argument, None));
        } else if let Some(token) = element.token()
            && token.kind() == SyntaxKind::COMMA
            && let Some((_, comma)) = items.last_mut()
        {
            *comma = Some(token);
        }
    }
    items
}

fn print_validated_call_args_single_line(
    args: Validated<'_, raw_ast::CallArgs>,
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    let open = args.l_paren_token();
    let close = args.r_paren_token();
    let items = validated_call_args_items(args);
    printer.print_raw_token(&open);
    printer
        .try_print_trivia_single_line_squished(printer.trivia.get_for_range_split(open.span()).1)?;
    for (index, (argument, comma)) in items.iter().enumerate() {
        let (leading, trailing) = printer.trivia.get_for_element(argument);
        printer.try_print_trivia_single_line_squished(leading)?;
        if argument
            .print(Shape::unlimited_single_line(), printer)
            .multi_lined
        {
            return None;
        }
        printer.try_print_trivia_single_line_squished(trailing)?;
        if index + 1 < items.len() {
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
            let (comma_leading, comma_trailing) = printer.trivia.get_for_range_split(comma.span());
            printer.try_print_trivia_single_line_squished(comma_leading)?;
            printer.try_print_trivia_single_line_squished(comma_trailing)?;
        }
    }
    printer.try_print_trivia_single_line_squished(
        printer.trivia.get_for_range_split(close.span()).0,
    )?;
    printer.print_raw_token(&close);
    (printer.output.len() <= shape.width).then(PrintInfo::default_single_line)
}

fn print_validated_call_args_hug(
    args: Validated<'_, raw_ast::CallArgs>,
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    let items = validated_call_args_items(args);
    let ((last, last_comma), initial) = items.split_last()?;
    if !validated_call_arg_is_huggable(*last) {
        return None;
    }
    let open = args.l_paren_token();
    let close = args.r_paren_token();
    printer.print_raw_token(&open);
    printer
        .try_print_trivia_single_line_squished(printer.trivia.get_for_range_split(open.span()).1)?;
    for (argument, comma) in initial {
        let (leading, trailing) = printer.trivia.get_for_element(argument);
        printer.try_print_trivia_single_line_squished(leading)?;
        if argument
            .print(Shape::unlimited_single_line(), printer)
            .multi_lined
        {
            return None;
        }
        printer.try_print_trivia_single_line_squished(trailing)?;
        if let Some(comma) = comma {
            let (comma_leading, comma_trailing) = printer.trivia.get_for_range_split(comma.span());
            printer.try_print_trivia_single_line_squished(comma_leading)?;
            printer.print_raw_token(comma);
            printer.try_print_trivia_single_line_squished(comma_trailing)?;
        } else {
            printer.print_str(",");
        }
        printer.print_str(" ");
    }
    let (last_leading, last_trailing) = printer.trivia.get_for_element(last);
    printer.try_print_trivia_single_line_squished(last_leading)?;
    let first_line_offset = shape.first_line_offset + printer.current_line_len();
    last.print(
        Shape {
            width: printer
                .config
                .line_width
                .saturating_sub(shape.indent + first_line_offset),
            indent: shape.indent,
            first_line_offset,
        },
        printer,
    );
    printer.try_print_trivia_single_line_squished(last_trailing)?;
    if let Some(comma) = last_comma {
        let (comma_leading, comma_trailing) = printer.trivia.get_for_range_split(comma.span());
        printer.try_print_trivia_single_line_squished(comma_leading)?;
        printer.try_print_trivia_single_line_squished(comma_trailing)?;
    }
    printer.try_print_trivia_single_line_squished(
        printer.trivia.get_for_range_split(close.span()).0,
    )?;
    printer.print_raw_token(&close);
    Some(PrintInfo::default_multi_lined())
}

fn print_validated_call_args_multi_line(
    args: Validated<'_, raw_ast::CallArgs>,
    shape: &Shape,
    printer: &mut Printer,
) -> PrintInfo {
    let open = args.l_paren_token();
    let close = args.r_paren_token();
    let inner_indent = shape.indent + printer.config.indent_width;
    let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
    printer.print_raw_token(&open);
    printer.print_trivia_all_trailing_for(open.span());
    printer.print_newline();
    for (argument, comma) in validated_call_args_items(args) {
        printer.print_trivia_all_leading_with_newline_for(argument.leftmost_token(), inner_indent);
        printer.print_spaces(inner_indent);
        argument.print(inner_shape.clone(), printer);
        if let Some(comma) = comma {
            printer.print_raw_token(&comma);
            printer.print_trivia_all_trailing_for(comma.span());
        } else {
            printer.print_str(",");
            printer.print_trivia_all_trailing_for(argument.rightmost_token());
        }
        printer.print_newline();
    }
    printer.print_trivia_all_leading_with_newline_for(close.span(), inner_indent);
    printer.print_spaces(shape.indent);
    printer.print_raw_token(&close);
    PrintInfo::default_multi_lined()
}

impl Printable for Validated<'_, raw_ast::CallArgs> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| print_validated_call_args_single_line(*self, &shape, p))
            .or_else(|| {
                printer.try_sub_printer(|p| print_validated_call_args_hug(*self, &shape, p))
            })
            .unwrap_or_else(|| print_validated_call_args_multi_line(*self, &shape, printer))
    }

    fn leftmost_token(&self) -> TextRange {
        self.l_paren_token().span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.r_paren_token().span()
    }
}

#[derive(Clone, Copy)]
struct ValidatedIndexArgs<'tree> {
    open: ValidatedSyntaxToken,
    index: Validated<'tree, raw_ast::ExprNode>,
    close: ValidatedSyntaxToken,
}

fn print_validated_index_single_line(
    args: ValidatedIndexArgs<'_>,
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    printer.print_raw_token(&args.open);
    printer.try_print_trivia_single_line_squished(
        printer.trivia.get_for_range_split(args.open.span()).1,
    )?;
    let (leading, trailing) = printer.trivia.get_for_element(&args.index);
    printer.try_print_trivia_single_line_squished(leading)?;
    if args
        .index
        .print(Shape::unlimited_single_line(), printer)
        .multi_lined
    {
        return None;
    }
    printer.try_print_trivia_single_line_squished(trailing)?;
    printer.try_print_trivia_single_line_squished(
        printer.trivia.get_for_range_split(args.close.span()).0,
    )?;
    printer.print_raw_token(&args.close);
    (printer.output.len() <= shape.width).then(PrintInfo::default_single_line)
}

fn print_validated_index_multi_line(
    args: ValidatedIndexArgs<'_>,
    shape: &Shape,
    printer: &mut Printer,
) -> PrintInfo {
    let inner_indent = shape.indent + printer.config.indent_width;
    printer.print_raw_token(&args.open);
    printer.print_trivia_all_trailing_for(args.open.span());
    printer.print_newline();
    let (leading, trailing) = printer.trivia.get_for_element(&args.index);
    printer.print_trivia_with_newline(leading.trim_blanks(), inner_indent);
    printer.print_spaces(inner_indent);
    args.index.print(
        Shape::standalone(printer.config.line_width, inner_indent),
        printer,
    );
    printer.print_trivia_trailing(trailing);
    printer.print_newline();
    printer.print_trivia_with_newline(
        printer
            .trivia
            .get_for_range_split(args.close.span())
            .0
            .trim_blanks(),
        inner_indent,
    );
    printer.print_spaces(shape.indent);
    printer.print_raw_token(&args.close);
    PrintInfo::default_multi_lined()
}

impl Printable for ValidatedIndexArgs<'_> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| print_validated_index_single_line(*self, &shape, p))
            .unwrap_or_else(|| print_validated_index_multi_line(*self, &shape, printer))
    }

    fn leftmost_token(&self) -> TextRange {
        self.open.span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.close.span()
    }
}

fn validated_index_args(index: Validated<'_, raw_ast::IndexExpr>) -> ValidatedIndexArgs<'_> {
    ValidatedIndexArgs {
        open: index.l_bracket_token(),
        index: index.index(),
        close: index.r_bracket_token(),
    }
}

fn validated_optional_index_args(
    index: Validated<'_, raw_ast::OptionalIndexExpr>,
) -> ValidatedIndexArgs<'_> {
    ValidatedIndexArgs {
        open: index.l_bracket_token(),
        index: index.index(),
        close: index.r_bracket_token(),
    }
}

fn print_validated_call_direct(
    call: Validated<'_, raw_ast::CallExpr>,
    shape: &Shape,
    printer: &mut Printer,
) -> PrintInfo {
    let line_before = printer.current_line_len();
    let callee = validated_peel_to_needed_paren(call.callee(), printer.trivia, false);
    let mut info = callee.print(shape.clone(), printer);
    let args_shape = Shape {
        first_line_offset: shape.first_line_offset
            + printer.current_line_len().saturating_sub(line_before),
        ..shape.clone()
    };
    info.multi_lined |= call.call_args().print(args_shape, printer).multi_lined;
    info
}

fn print_validated_index_direct(
    index: Validated<'_, raw_ast::IndexExpr>,
    shape: &Shape,
    printer: &mut Printer,
) -> PrintInfo {
    let base = validated_peel_to_needed_paren(index.base(), printer.trivia, false);
    let mut probe = printer.sub_printer();
    let base_info = base.print(Shape::unlimited_single_line(), &mut probe);
    let args_info = validated_index_args(index).print(Shape::unlimited_single_line(), &mut probe);
    if !base_info.multi_lined && !args_info.multi_lined && probe.output.len() <= shape.width {
        printer.append_from_printer(probe);
        PrintInfo::default_single_line()
    } else {
        base.print(shape.clone(), printer);
        print_validated_index_multi_line(validated_index_args(index), shape, printer)
    }
}

#[derive(Clone, Copy)]
enum ValidatedPrintChainItem<'tree> {
    Field(ValidatedSyntaxToken, ValidatedSyntaxToken),
    OptionalField(ValidatedSyntaxToken, ValidatedSyntaxToken),
    Index(ValidatedIndexArgs<'tree>),
    OptionalIndex(ValidatedSyntaxToken, ValidatedIndexArgs<'tree>),
    Call(Validated<'tree, raw_ast::CallArgs>),
    OptionalCall(ValidatedSyntaxToken, Validated<'tree, raw_ast::CallArgs>),
    Generic(Validated<'tree, raw_ast::GenericArgs>),
}

#[derive(Clone, Copy)]
enum ValidatedPrintChainFirst<'tree> {
    Expr(Validated<'tree, raw_ast::ExprNode>),
    Token(ValidatedSyntaxToken),
}

struct ValidatedPrintChain<'tree> {
    first: ValidatedPrintChainFirst<'tree>,
    members: Vec<ValidatedPrintChainItem<'tree>>,
}

impl<'tree> ValidatedPrintChain<'tree> {
    fn new(from: Validated<'tree, raw_ast::ExprNode>, trivia: &TriviaInfo) -> Self {
        let from = validated_peel_to_needed_paren(from, trivia, false);
        match from.as_variant() {
            ValidatedExprNode::PathExpr(path) => {
                let mut first = None;
                let mut members = Vec::new();
                Self::flatten_path(path, &mut first, &mut members);
                Self {
                    first: ValidatedPrintChainFirst::Token(
                        first.expect("validated path expression head"),
                    ),
                    members,
                }
            }
            ValidatedExprNode::CallExpr(call) => {
                let mut chain = Self::new(call.callee(), trivia);
                if chain.members.is_empty() {
                    Self {
                        first: ValidatedPrintChainFirst::Expr(from),
                        members: Vec::new(),
                    }
                } else {
                    chain
                        .members
                        .push(ValidatedPrintChainItem::Call(call.call_args()));
                    chain
                }
            }
            ValidatedExprNode::IndexExpr(index) => {
                let mut chain = Self::new(index.base(), trivia);
                if chain.members.is_empty() {
                    Self {
                        first: ValidatedPrintChainFirst::Expr(from),
                        members: Vec::new(),
                    }
                } else {
                    chain
                        .members
                        .push(ValidatedPrintChainItem::Index(validated_index_args(index)));
                    chain
                }
            }
            ValidatedExprNode::FieldAccessExpr(access) => {
                let mut chain = Self::new(access.base(), trivia);
                chain.members.push(ValidatedPrintChainItem::Field(
                    access.dot_token(),
                    access.name_token(),
                ));
                chain
            }
            ValidatedExprNode::OptionalFieldAccessExpr(access) => {
                let mut chain = Self::new(access.base(), trivia);
                chain.members.push(ValidatedPrintChainItem::OptionalField(
                    access.question_dot_token(),
                    access.name_token(),
                ));
                chain
            }
            ValidatedExprNode::OptionalIndexExpr(index) => {
                let mut chain = Self::new(index.base(), trivia);
                chain.members.push(ValidatedPrintChainItem::OptionalIndex(
                    index.question_dot_token(),
                    validated_optional_index_args(index),
                ));
                chain
            }
            ValidatedExprNode::OptionalCallExpr(call) => {
                let mut chain = Self::new(call.callee(), trivia);
                chain.members.push(ValidatedPrintChainItem::OptionalCall(
                    call.question_dot_token(),
                    call.call_args(),
                ));
                chain
            }
            _ => Self {
                first: ValidatedPrintChainFirst::Expr(from),
                members: Vec::new(),
            },
        }
    }

    fn flatten_path(
        path: Validated<'tree, raw_ast::PathExpr>,
        first: &mut Option<ValidatedSyntaxToken>,
        members: &mut Vec<ValidatedPrintChainItem<'tree>>,
    ) {
        let mut separator = None;
        for element in path.direct_elements() {
            if let Some(nested) = element.node::<raw_ast::PathExpr>() {
                Self::flatten_path(nested, first, members);
            } else if let Some(args) = element.node::<raw_ast::GenericArgs>() {
                members.push(ValidatedPrintChainItem::Generic(args));
            } else if let Some(token) = element.token() {
                if matches!(token.kind(), SyntaxKind::DOT | SyntaxKind::DOUBLE_COLON) {
                    separator = Some(token);
                } else if let Some(separator) = separator.take() {
                    members.push(ValidatedPrintChainItem::Field(separator, token));
                } else if first.is_none() {
                    *first = Some(token);
                }
            }
        }
    }

    fn print_first(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let ValidatedPrintChainFirst::Expr(first) = self.first else {
            let ValidatedPrintChainFirst::Token(token) = self.first else {
                unreachable!()
            };
            printer.print_raw_token(&token);
            return PrintInfo::default_single_line();
        };
        match first.as_variant() {
            ValidatedExprNode::CallExpr(call) => print_validated_call_direct(call, &shape, printer),
            ValidatedExprNode::IndexExpr(index) => {
                print_validated_index_direct(index, &shape, printer)
            }
            ValidatedExprNode::FieldAccessExpr(_)
            | ValidatedExprNode::OptionalFieldAccessExpr(_)
            | ValidatedExprNode::OptionalIndexExpr(_)
            | ValidatedExprNode::OptionalCallExpr(_) => {
                unreachable!("postfix chain must be flattened")
            }
            _ => first.print(shape, printer),
        }
    }

    const fn is_plain(item: &ValidatedPrintChainItem<'_>) -> bool {
        matches!(
            item,
            ValidatedPrintChainItem::Field(..)
                | ValidatedPrintChainItem::OptionalField(..)
                | ValidatedPrintChainItem::Generic(..)
        )
    }

    fn print_plain(item: ValidatedPrintChainItem<'_>, printer: &mut Printer) {
        match item {
            ValidatedPrintChainItem::Field(dot, name)
            | ValidatedPrintChainItem::OptionalField(dot, name) => {
                printer.print_raw_token(&dot);
                printer.print_raw_token(&name);
            }
            ValidatedPrintChainItem::Generic(args) => {
                args.print(Shape::unlimited_single_line(), printer);
            }
            _ => unreachable!("plain chain item"),
        }
    }

    fn item_width(item: ValidatedPrintChainItem<'_>, printer: &Printer<'_>) -> Option<usize> {
        match item {
            ValidatedPrintChainItem::Field(dot, name)
            | ValidatedPrintChainItem::OptionalField(dot, name) => {
                Some(usize::from(dot.span().len() + name.span().len()))
            }
            ValidatedPrintChainItem::Generic(args) => {
                let mut probe = printer.sub_printer();
                let info = args.print(Shape::unlimited_single_line(), &mut probe);
                (!info.multi_lined && !probe.output.contains('\n')).then_some(probe.output.len())
            }
            _ => {
                let mut probe = printer.sub_printer();
                item.print_single(&Shape::unlimited_single_line(), &mut probe)?;
                (!probe.output.contains('\n')).then_some(probe.output.len())
            }
        }
    }

    fn group_width(group: &[ValidatedPrintChainItem<'_>], printer: &Printer<'_>) -> Option<usize> {
        group
            .iter()
            .copied()
            .map(|item| Self::item_width(item, printer))
            .sum()
    }

    fn print_non_plain(
        item: ValidatedPrintChainItem<'_>,
        chain_indent: usize,
        remaining: &mut usize,
        printer: &mut Printer,
    ) -> bool {
        let shape = Shape {
            width: *remaining,
            indent: chain_indent,
            first_line_offset: printer.current_line_len().saturating_sub(chain_indent),
        };
        let info = match item {
            ValidatedPrintChainItem::Index(args) => args.print(shape, printer),
            ValidatedPrintChainItem::OptionalIndex(question_dot, args) => {
                printer.print_raw_token(&question_dot);
                args.print(shape, printer)
            }
            ValidatedPrintChainItem::Call(args) => args.print(shape, printer),
            ValidatedPrintChainItem::OptionalCall(question_dot, args) => {
                printer.print_raw_token(&question_dot);
                args.print(shape, printer)
            }
            _ => unreachable!("non-plain chain item"),
        };
        *remaining = printer.current_line_remaining_width();
        info.multi_lined
    }

    fn try_members_single_line(
        &self,
        members: &[ValidatedPrintChainItem<'_>],
        shape: &Shape,
        printer: &mut Printer,
    ) -> Option<()> {
        if self
            .print_first(Shape::unlimited_single_line(), printer)
            .multi_lined
        {
            return None;
        }
        for item in members {
            if printer.output.len() > shape.width {
                return None;
            }
            item.print_single(shape, printer)?;
        }
        Some(())
    }

    fn try_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        self.try_members_single_line(&self.members, shape, printer)?;
        (printer.output.len() <= shape.width).then(PrintInfo::default_single_line)
    }

    fn try_hug(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        let (last, prefix) = self.members.split_last()?;
        let (question_dot, args) = match *last {
            ValidatedPrintChainItem::Call(args) => (None, args),
            ValidatedPrintChainItem::OptionalCall(question_dot, args) => (Some(question_dot), args),
            _ => return None,
        };
        self.try_members_single_line(prefix, shape, printer)?;
        if printer.output.len() > shape.width {
            return None;
        }
        if let Some(question_dot) = question_dot {
            printer.print_raw_token(&question_dot);
        }
        print_validated_call_args_hug(
            args,
            &Shape {
                width: shape.width,
                indent: shape.indent,
                first_line_offset: shape.first_line_offset,
            },
            printer,
        )
    }

    fn try_tail_broken(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        let (last, prefix) = self.members.split_last()?;
        let question_dot = match *last {
            ValidatedPrintChainItem::Call(_) | ValidatedPrintChainItem::Index(_) => None,
            ValidatedPrintChainItem::OptionalCall(question_dot, _)
            | ValidatedPrintChainItem::OptionalIndex(question_dot, _) => Some(question_dot),
            _ => return None,
        };
        self.try_members_single_line(prefix, shape, printer)?;
        if let Some(question_dot) = question_dot {
            printer.print_raw_token(&question_dot);
        }
        if printer.output.len() > shape.width {
            return None;
        }
        let prefix_width = printer.output.len();
        let args_shape = Shape {
            width: shape.width.saturating_sub(prefix_width),
            indent: shape.indent,
            first_line_offset: shape.first_line_offset + prefix_width,
        };
        let info = match *last {
            ValidatedPrintChainItem::Call(args)
            | ValidatedPrintChainItem::OptionalCall(_, args) => args.print(args_shape, printer),
            ValidatedPrintChainItem::Index(args)
            | ValidatedPrintChainItem::OptionalIndex(_, args) => args.print(args_shape, printer),
            _ => unreachable!("checked call or index"),
        };
        let first_line = printer.output.find('\n').unwrap_or(printer.output.len());
        (first_line <= shape.width).then_some(info)
    }

    fn print_multi_line(&self, shape: &Shape, printer: &mut Printer) -> PrintInfo {
        let first_single_line = !self.print_first(shape.clone(), printer).multi_lined;
        let mut multi_lined = !first_single_line;
        let chain_indent = shape.indent + printer.config.indent_width;
        let mut remaining = printer.current_line_remaining_width();
        let mut rest = self.members.as_slice();

        while let Some((item, tail)) = rest.split_first() {
            if Self::is_plain(item) {
                break;
            }
            multi_lined |= Self::print_non_plain(*item, chain_indent, &mut remaining, printer);
            rest = tail;
        }

        let plain_len = rest.iter().take_while(|item| Self::is_plain(item)).count();
        let path_len = if plain_len == rest.len() {
            plain_len
        } else {
            plain_len.saturating_sub(1)
        };
        for item in rest[..path_len].iter().copied() {
            Self::print_plain(item, printer);
        }
        rest = &rest[path_len..];
        remaining = printer.current_line_remaining_width();

        let mut first_group = true;
        while !rest.is_empty() {
            let plain = rest.iter().take_while(|item| Self::is_plain(item)).count();
            let callish = rest[plain..]
                .iter()
                .take_while(|item| !Self::is_plain(item))
                .count();
            let (group, tail) = rest.split_at(plain + callish);
            rest = tail;
            let glue = if plain == 0 {
                true
            } else if first_group && first_single_line {
                Self::group_width(group, printer).is_some_and(|width| width <= remaining)
            } else {
                false
            };
            if !glue {
                printer.print_newline();
                printer.print_spaces(chain_indent);
                remaining = printer.config.line_width.saturating_sub(chain_indent);
                multi_lined = true;
            }
            for item in group.iter().copied() {
                if Self::is_plain(&item) {
                    Self::print_plain(item, printer);
                    remaining = printer.current_line_remaining_width();
                } else {
                    multi_lined |=
                        Self::print_non_plain(item, chain_indent, &mut remaining, printer);
                }
            }
            first_group = false;
        }
        PrintInfo { multi_lined }
    }
}

impl ValidatedPrintChainItem<'_> {
    fn print_single(&self, shape: &Shape, printer: &mut Printer) -> Option<()> {
        match *self {
            Self::Field(dot, name) | Self::OptionalField(dot, name) => {
                printer.print_raw_token(&dot);
                printer.print_raw_token(&name);
            }
            Self::Index(args) => {
                print_validated_index_single_line(args, shape, printer)?;
            }
            Self::OptionalIndex(question_dot, args) => {
                printer.print_raw_token(&question_dot);
                print_validated_index_single_line(args, shape, printer)?;
            }
            Self::Call(args) => {
                print_validated_call_args_single_line(args, shape, printer)?;
            }
            Self::OptionalCall(question_dot, args) => {
                printer.print_raw_token(&question_dot);
                print_validated_call_args_single_line(args, shape, printer)?;
            }
            Self::Generic(args) => {
                args.print(Shape::unlimited_single_line(), printer);
            }
        }
        Some(())
    }
}

impl Printable for ValidatedPrintChain<'_> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|probe| self.try_single_line(&shape, probe))
            .or_else(|| printer.try_sub_printer(|probe| self.try_hug(&shape, probe)))
            .or_else(|| printer.try_sub_printer(|probe| self.try_tail_broken(&shape, probe)))
            .unwrap_or_else(|| self.print_multi_line(&shape, printer))
    }

    fn leftmost_token(&self) -> TextRange {
        match self.first {
            ValidatedPrintChainFirst::Expr(first) => first.leftmost_token(),
            ValidatedPrintChainFirst::Token(token) => token.span(),
        }
    }

    fn rightmost_token(&self) -> TextRange {
        match self.members.last().copied() {
            Some(
                ValidatedPrintChainItem::Field(_, name)
                | ValidatedPrintChainItem::OptionalField(_, name),
            ) => name.span(),
            Some(
                ValidatedPrintChainItem::Index(args)
                | ValidatedPrintChainItem::OptionalIndex(_, args),
            ) => args.close.span(),
            Some(
                ValidatedPrintChainItem::Call(args)
                | ValidatedPrintChainItem::OptionalCall(_, args),
            ) => args.rightmost_token(),
            Some(ValidatedPrintChainItem::Generic(args)) => args.rightmost_token(),
            None => match self.first {
                ValidatedPrintChainFirst::Expr(first) => first.rightmost_token(),
                ValidatedPrintChainFirst::Token(token) => token.span(),
            },
        }
    }
}

fn validated_binary_effective_left<'tree>(
    binary: Validated<'tree, raw_ast::BinaryExpr>,
    trivia: &TriviaInfo,
) -> Validated<'tree, raw_ast::ExprNode> {
    let left = binary.lhs();
    let Some(row) = validated_binary_row(validated_binary_operator(binary)) else {
        return left;
    };
    let peeled = validated_peel_transparent_parens(left, trivia);
    let ValidatedExprNode::BinaryExpr(inner) = peeled.as_variant() else {
        return left;
    };
    if validated_binary_row(validated_binary_operator(inner)) == Some(row) {
        peeled
    } else {
        left
    }
}

fn validated_binary_members<'tree>(
    binary: Validated<'tree, raw_ast::BinaryExpr>,
    trivia: &TriviaInfo,
) -> (
    Validated<'tree, raw_ast::ExprNode>,
    Vec<(ValidatedBinaryOperator, Validated<'tree, raw_ast::ExprNode>)>,
) {
    let operator = validated_binary_operator(binary);
    let left = validated_binary_effective_left(binary, trivia);
    let right = binary.rhs();
    let mut members = Vec::new();
    let Some(group) = validated_binary_group(operator) else {
        members.push((operator, right));
        return (left, members);
    };
    let left_binary = match left.as_variant() {
        ValidatedExprNode::BinaryExpr(node)
            if validated_binary_group(validated_binary_operator(node)) == Some(group) =>
        {
            Some(node)
        }
        _ => None,
    };
    let right_binary = match right.as_variant() {
        ValidatedExprNode::BinaryExpr(node)
            if validated_binary_group(validated_binary_operator(node)) == Some(group) =>
        {
            Some(node)
        }
        _ => None,
    };
    match (left_binary, right_binary) {
        (Some(left_binary), Some(right_binary)) => {
            let (first, left_rest) = validated_binary_members(left_binary, trivia);
            let (right_first, right_rest) = validated_binary_members(right_binary, trivia);
            members.extend(left_rest);
            members.push((operator, right_first));
            members.extend(right_rest);
            (first, members)
        }
        (Some(left_binary), None) => {
            let (first, left_rest) = validated_binary_members(left_binary, trivia);
            members.extend(left_rest);
            members.push((operator, right));
            (first, members)
        }
        (None, Some(right_binary)) => {
            let (right_first, right_rest) = validated_binary_members(right_binary, trivia);
            members.push((operator, right_first));
            members.extend(right_rest);
            (left, members)
        }
        (None, None) => {
            members.push((operator, right));
            (left, members)
        }
    }
}

fn print_validated_binary_single_line(
    binary: Validated<'_, raw_ast::BinaryExpr>,
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    let left = validated_binary_effective_left(binary, printer.trivia);
    let right = binary.rhs();
    let operator = validated_binary_operator(binary);
    if left
        .print(Shape::unlimited_single_line(), printer)
        .multi_lined
    {
        return None;
    }
    let mut left_trivia = printer
        .try_print_trivia_single_line_squished(printer.trivia.get_trailing_for_element(&left))?;
    let (operator_leading, operator_trailing) = printer.trivia.get_for_range_split(operator.span());
    left_trivia += printer.print_trivia_squished(operator_leading);
    if left_trivia == 0 {
        printer.print_spaces(1);
    }
    operator.print(Shape::unlimited_single_line(), printer);
    let mut right_trivia = printer.print_trivia_squished(operator_trailing);
    right_trivia += printer.print_trivia_squished(printer.trivia.get_leading_for_element(&right));
    if right_trivia == 0 {
        printer.print_spaces(1);
    }
    if right
        .print(Shape::unlimited_single_line(), printer)
        .multi_lined
    {
        return None;
    }
    (printer.output.len() <= shape.width).then(PrintInfo::default_single_line)
}

fn print_validated_binary_multi_line(
    binary: Validated<'_, raw_ast::BinaryExpr>,
    shape: Shape,
    printer: &mut Printer,
) -> PrintInfo {
    let inner_indent = shape.indent + printer.config.indent_width;
    let (first, members) = validated_binary_members(binary, printer.trivia);
    first.print(shape, printer);
    printer.print_trivia_all_trailing_for(first.rightmost_token());
    let member_count = members.len();
    for (index, (operator, right)) in members.into_iter().enumerate() {
        printer.print_newline();
        printer.print_spaces(inner_indent);
        operator.print(Shape::unlimited_single_line(), printer);
        printer.print_str(" ");
        let operator_width = usize::from(operator.span().len());
        let inner_shape = Shape {
            width: printer
                .config
                .line_width
                .saturating_sub(inner_indent + operator_width + 1),
            indent: inner_indent,
            first_line_offset: operator_width + 1,
        };
        right.print(inner_shape, printer);
        if index + 1 < member_count {
            printer.print_trivia_all_trailing_for(right.rightmost_token());
        }
    }
    PrintInfo::default_multi_lined()
}

impl Printable for Validated<'_, raw_ast::BinaryExpr> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| print_validated_binary_single_line(*self, &shape, p))
            .unwrap_or_else(|| print_validated_binary_multi_line(*self, shape, printer))
    }

    fn leftmost_token(&self) -> TextRange {
        self.lhs().leftmost_token()
    }

    fn rightmost_token(&self) -> TextRange {
        self.rhs().rightmost_token()
    }
}

fn validated_paren_is_transparent(
    paren: Validated<'_, raw_ast::ParenExpr>,
    trivia: &TriviaInfo,
) -> bool {
    let open = paren.l_paren_token();
    let close = paren.r_paren_token();
    let expr = paren.expr_node();
    let (open_leading, open_trailing) = trivia.get_for_range_split(open.span());
    let (close_leading, close_trailing) = trivia.get_for_range_split(close.span());
    let (expr_leading, expr_trailing) = trivia.get_for_element(&expr);
    open_leading.is_empty()
        && open_trailing.is_empty()
        && close_leading.is_empty()
        && close_trailing.is_empty()
        && expr_leading.is_empty()
        && expr_trailing.is_empty()
}

fn validated_has_optional_chain_link(expr: Validated<'_, raw_ast::ExprNode>) -> bool {
    match expr.as_variant() {
        ValidatedExprNode::OptionalFieldAccessExpr(_)
        | ValidatedExprNode::OptionalIndexExpr(_)
        | ValidatedExprNode::OptionalCallExpr(_) => true,
        ValidatedExprNode::CallExpr(call) => validated_has_optional_chain_link(call.callee()),
        ValidatedExprNode::IndexExpr(index) => validated_has_optional_chain_link(index.base()),
        ValidatedExprNode::FieldAccessExpr(access) => {
            validated_has_optional_chain_link(access.base())
        }
        ValidatedExprNode::ParenExpr(paren) => validated_has_optional_chain_link(paren.expr_node()),
        _ => false,
    }
}

fn validated_binds_as_postfix_operand(expr: Validated<'_, raw_ast::ExprNode>) -> bool {
    match expr.as_variant() {
        ValidatedExprNode::CallExpr(_)
        | ValidatedExprNode::IndexExpr(_)
        | ValidatedExprNode::FieldAccessExpr(_) => !validated_has_optional_chain_link(expr),
        ValidatedExprNode::PathExpr(_)
        | ValidatedExprNode::EnvAccessExpr(_)
        | ValidatedExprNode::ArrayLiteral(_)
        | ValidatedExprNode::RawStringLiteral(_)
        | ValidatedExprNode::BacktickStringLiteral(_)
        | ValidatedExprNode::ByteStringLiteral(_)
        | ValidatedExprNode::StringLiteral(_) => true,
        _ => false,
    }
}

fn validated_peel_to_needed_paren<'tree>(
    mut expr: Validated<'tree, raw_ast::ExprNode>,
    trivia: &TriviaInfo,
    unary: bool,
) -> Validated<'tree, raw_ast::ExprNode> {
    loop {
        let ValidatedExprNode::ParenExpr(paren) = expr.as_variant() else {
            return expr;
        };
        if !validated_paren_is_transparent(paren, trivia) {
            return expr;
        }
        let inner = paren.expr_node();
        let stands_alone = validated_binds_as_postfix_operand(inner)
            || matches!(inner.as_variant(), ValidatedExprNode::ParenExpr(_))
            || (unary
                && matches!(
                    inner.as_variant(),
                    ValidatedExprNode::LiteralExpr(_) | ValidatedExprNode::StringLiteral(_)
                ));
        if !stands_alone {
            return expr;
        }
        expr = inner;
    }
}

impl Printable for Validated<'_, raw_ast::UnaryExpr> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let operator = self
            .direct_elements()
            .find_map(|element| element.token())
            .expect("validated unary operator");
        printer.print_raw_token(&operator);
        validated_peel_to_needed_paren(self.operand(), printer.trivia, true).print(shape, printer)
    }

    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.operand().rightmost_token()
    }
}

impl Printable for Validated<'_, raw_ast::ParenExpr> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| print_validated_paren_single_line(self, &shape, p))
            .unwrap_or_else(|| print_validated_paren_multi_line(self, &shape, printer))
    }

    fn leftmost_token(&self) -> TextRange {
        self.l_paren_token().text_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.r_paren_token().text_range()
    }
}

fn print_validated_paren_single_line(
    paren: &Validated<'_, raw_ast::ParenExpr>,
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    let open = paren.l_paren_token();
    let expr = paren.expr_node();
    let close = paren.r_paren_token();
    printer.print_raw_token(&open);
    printer
        .try_print_trivia_single_line_squished(printer.trivia.get_for_range_split(open.span()).1)?;
    let (expr_leading, expr_trailing) = printer.trivia.get_for_element(&expr);
    printer.try_print_trivia_single_line_squished(expr_leading)?;
    if expr
        .print(Shape::unlimited_single_line(), printer)
        .multi_lined
    {
        return None;
    }
    printer.try_print_trivia_single_line_squished(expr_trailing)?;
    printer.try_print_trivia_single_line_squished(
        printer.trivia.get_for_range_split(close.span()).0,
    )?;
    printer.print_raw_token(&close);
    (printer.output.len() <= shape.width).then(PrintInfo::default_single_line)
}

fn print_validated_paren_multi_line(
    paren: &Validated<'_, raw_ast::ParenExpr>,
    shape: &Shape,
    printer: &mut Printer,
) -> PrintInfo {
    let open = paren.l_paren_token();
    let expr = paren.expr_node();
    let close = paren.r_paren_token();
    let inner_shape = Shape {
        width: shape.width.saturating_sub(printer.config.indent_width),
        indent: shape.indent + printer.config.indent_width,
        first_line_offset: 0,
    };
    printer.print_raw_token(&open);
    printer.print_trivia_all_trailing_for(open.span());
    printer.print_newline();
    let (expr_leading, expr_trailing) = printer.trivia.get_for_element(&expr);
    printer.print_trivia_with_newline(expr_leading.trim_blanks(), inner_shape.indent);
    printer.print_spaces(inner_shape.indent);
    expr.print(inner_shape.clone(), printer);
    printer.print_trivia_trailing(expr_trailing);
    printer.print_newline();
    let (close_leading, _) = printer.trivia.get_for_range_split(close.span());
    printer.print_trivia_with_newline(close_leading.trim_blanks(), inner_shape.indent);
    printer.print_spaces(shape.indent);
    printer.print_raw_token(&close);
    PrintInfo::default_multi_lined()
}

impl Printable for Validated<'_, raw_ast::PathExpr> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut info = PrintInfo::default_single_line();
        for element in self.direct_elements() {
            if let Some(path) = element.node::<raw_ast::PathExpr>() {
                info.multi_lined |= path.print(shape.clone(), printer).multi_lined;
            } else if let Some(args) = element.node::<raw_ast::GenericArgs>() {
                info.multi_lined |= args.print(shape.clone(), printer).multi_lined;
            } else if let Some(token) = element.token() {
                printer.print_raw_token(&token);
            }
        }
        info
    }

    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for Validated<'_, raw_ast::GenericArgs> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.less_token());
        let mut arguments = self.direct_elements().filter_map(|element| {
            element
                .node::<raw_ast::TypeExpr>()
                .map(ValidatedGenericArg::Type)
                .or_else(|| {
                    element
                        .node::<raw_ast::UnreflectArg>()
                        .map(ValidatedGenericArg::Unreflect)
                })
        });
        if let Some(first) = arguments.next() {
            first.print(shape.clone(), printer);
            for argument in arguments {
                printer.print_str(", ");
                argument.print(shape.clone(), printer);
            }
        }
        printer.print_raw_token(&self.greater_token());
        PrintInfo::default_single_line()
    }

    fn leftmost_token(&self) -> TextRange {
        self.less_token().text_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.greater_token().text_range()
    }
}

enum ValidatedGenericArg<'tree> {
    Type(Validated<'tree, raw_ast::TypeExpr>),
    Unreflect(Validated<'tree, raw_ast::UnreflectArg>),
}

impl Printable for ValidatedGenericArg<'_> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            Self::Type(ty) => ty.print(shape, printer),
            Self::Unreflect(argument) => argument.print(shape, printer),
        }
    }

    fn leftmost_token(&self) -> TextRange {
        match self {
            Self::Type(ty) => ty.leftmost_token(),
            Self::Unreflect(argument) => argument.leftmost_token(),
        }
    }

    fn rightmost_token(&self) -> TextRange {
        match self {
            Self::Type(ty) => ty.rightmost_token(),
            Self::Unreflect(argument) => argument.rightmost_token(),
        }
    }
}

impl Printable for Validated<'_, raw_ast::UnreflectArg> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.name_token());
        printer.print_raw_token(&self.l_paren_token());
        let info = self.value().print(shape, printer);
        printer.print_raw_token(&self.r_paren_token());
        info
    }

    fn leftmost_token(&self) -> TextRange {
        self.name_token().text_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.r_paren_token().text_range()
    }
}
