/// Does not correspond to a specific [`SyntaxKind`], but contains all possible statements.
//
// `For(ForStmt)` is the largest variant (~720 bytes); the next-largest sits
// well below it. The size difference is acknowledged here rather than
// boxed because `Statement` is constructed transiently during formatting,
// not stored at scale.
use baml_db::baml_compiler_syntax::{
    SyntaxKind, ast as raw_ast,
    validated::{Validated, ValidatedBlockItem, ValidatedSyntaxToken},
};
use rowan::TextRange;

use crate::{
    ast::Token,
    printer::{PrintInfo, Printable, Printer, Shape},
    trivia_classifier::TriviaSliceExt,
};

#[derive(Clone, Copy)]
enum ValidatedBlockEntry<'tree> {
    Item {
        item: Validated<'tree, raw_ast::BlockItem>,
        semicolon: Option<ValidatedSyntaxToken>,
    },
    Semicolon(ValidatedSyntaxToken),
}

fn validated_block_entries(
    block: Validated<'_, raw_ast::BlockExpr>,
) -> Vec<ValidatedBlockEntry<'_>> {
    let mut entries = Vec::new();
    for element in block.direct_elements() {
        if let Some(item) = element.node::<raw_ast::BlockItem>() {
            entries.push(ValidatedBlockEntry::Item {
                item,
                semicolon: None,
            });
        } else if let Some(token) = element.token()
            && token.kind() == SyntaxKind::SEMICOLON
        {
            if let Some(ValidatedBlockEntry::Item { item, semicolon }) = entries.last_mut()
                && item.cast::<raw_ast::ExprNode>().is_some()
                && semicolon.is_none()
            {
                *semicolon = Some(token);
            } else {
                entries.push(ValidatedBlockEntry::Semicolon(token));
            }
        }
    }
    entries
}

fn validated_expr_statement_needs_semicolon(expr: Validated<'_, raw_ast::ExprNode>) -> bool {
    !matches!(
        expr.as_variant(),
        baml_db::baml_compiler_syntax::validated::ValidatedExprNode::IfExpr(_)
            | baml_db::baml_compiler_syntax::validated::ValidatedExprNode::IfLetExpr(_)
            | baml_db::baml_compiler_syntax::validated::ValidatedExprNode::MatchExpr(_)
            | baml_db::baml_compiler_syntax::validated::ValidatedExprNode::LambdaExpr(_)
            | baml_db::baml_compiler_syntax::validated::ValidatedExprNode::SpawnExpr(_)
            | baml_db::baml_compiler_syntax::validated::ValidatedExprNode::TaggedTemplateExpr(_)
            | baml_db::baml_compiler_syntax::validated::ValidatedExprNode::UpcastExpr(_)
            | baml_db::baml_compiler_syntax::validated::ValidatedExprNode::QualifiedPathExpr(_)
            | baml_db::baml_compiler_syntax::validated::ValidatedExprNode::SpecExpr(_)
            | baml_db::baml_compiler_syntax::validated::ValidatedExprNode::ThrowExpr(_)
            | baml_db::baml_compiler_syntax::validated::ValidatedExprNode::AwaitExpr(_)
            | baml_db::baml_compiler_syntax::validated::ValidatedExprNode::ForExpr(_)
    )
}

impl Printable for ValidatedBlockEntry<'_> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            Self::Semicolon(semicolon) => {
                printer.print_raw_token(semicolon);
                PrintInfo::default_single_line()
            }
            Self::Item { item, semicolon } => {
                let info = item.print(shape, printer);
                if let Some(semicolon) = semicolon {
                    printer.print_trivia_squished(printer.trivia.get_trailing_for_element(item));
                    printer.print_trivia_squished(
                        printer.trivia.get_for_range_split(semicolon.span()).0,
                    );
                    printer.print_raw_token(semicolon);
                } else if item
                    .cast::<raw_ast::ExprNode>()
                    .is_some_and(validated_expr_statement_needs_semicolon)
                {
                    printer.print_str(";");
                }
                info
            }
        }
    }

    fn leftmost_token(&self) -> TextRange {
        match self {
            Self::Item { item, .. } => item.first_token_range(),
            Self::Semicolon(token) => token.span(),
        }
    }

    fn rightmost_token(&self) -> TextRange {
        match self {
            Self::Item {
                item: _,
                semicolon: Some(semicolon),
            } => semicolon.span(),
            Self::Item { item, .. } => item.last_token_range(),
            Self::Semicolon(token) => token.span(),
        }
    }
}

impl Printable for Validated<'_, raw_ast::BlockExpr> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let open = self.l_brace_token();
        let close = self.r_brace_token();
        let mut entries = validated_block_entries(*self);
        let tail = entries.last().copied().and_then(|entry| match entry {
            ValidatedBlockEntry::Item {
                item,
                semicolon: None,
            } => item.cast::<raw_ast::ExprNode>(),
            _ => None,
        });
        if tail.is_some() {
            entries.pop();
        }
        if entries.is_empty() && tail.is_none() {
            let (_, open_trailing) = printer.trivia.get_for_range_split(open.span());
            let (close_leading, _) = printer.trivia.get_for_range_split(close.span());
            if !open_trailing.iter().any(crate::EmittableTrivia::is_comment)
                && !close_leading.iter().any(crate::EmittableTrivia::is_comment)
            {
                printer.print_raw_token(&open);
                printer.print_raw_token(&close);
                return PrintInfo::default_single_line();
            }
        }
        printer.print_raw_token(&open);
        printer.print_trivia_all_trailing_for(open.span());
        printer.print_newline();
        let inner_indent = shape.indent + printer.config.indent_width;
        for (index, entry) in entries.iter().enumerate() {
            if index == 0 {
                let (leading, trailing) = printer.trivia.get_for_element(entry);
                printer.print_trivia_with_newline(leading.trim_leading_blanks(), inner_indent);
                printer.print_spaces(inner_indent);
                entry.print(
                    Shape::standalone(printer.config.line_width, inner_indent),
                    printer,
                );
                printer.print_trivia_trailing(trailing);
            } else {
                printer.print_standalone_with_trivia(entry, inner_indent);
            }
            printer.print_newline();
        }
        if let Some(tail) = tail {
            let (leading, trailing) = printer.trivia.get_for_element(&tail);
            let leading = if entries.is_empty() {
                leading.trim_leading_blanks()
            } else {
                leading
            };
            printer.print_trivia_with_newline(leading, inner_indent);
            printer.print_spaces(inner_indent);
            tail.print(
                Shape::standalone(printer.config.line_width, inner_indent),
                printer,
            );
            printer.print_trivia_trailing(trailing);
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

    fn leftmost_token(&self) -> TextRange {
        self.l_brace_token().span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.r_brace_token().span()
    }
}

impl Printable for Validated<'_, raw_ast::BlockItem> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self.as_variant() {
            ValidatedBlockItem::HeaderComment(node) => {
                printer.print_input_range_trimmed_start(node.text_range());
                PrintInfo::default_single_line()
            }
            ValidatedBlockItem::WhileStmt(node) => node.print(shape, printer),
            ValidatedBlockItem::WhileLetStmt(node) => node.print(shape, printer),
            ValidatedBlockItem::LetStmt(node) => node.print(shape, printer),
            ValidatedBlockItem::BreakStmt(node) => node.print(shape, printer),
            ValidatedBlockItem::ContinueStmt(node) => node.print(shape, printer),
            ValidatedBlockItem::ReturnStmt(node) => node.print(shape, printer),
            ValidatedBlockItem::TestExprDef(node) => node.print(shape, printer),
            ValidatedBlockItem::TestsetDef(node) => node.print(shape, printer),
            ValidatedBlockItem::ForExpr(node) => node.print(shape, printer),
            ValidatedBlockItem::LiteralExpr(_)
            | ValidatedBlockItem::BlockExpr(_)
            | ValidatedBlockItem::PathExpr(_)
            | ValidatedBlockItem::StringLiteral(_)
            | ValidatedBlockItem::RawStringLiteral(_)
            | ValidatedBlockItem::BacktickStringLiteral(_)
            | ValidatedBlockItem::ByteStringLiteral(_)
            | ValidatedBlockItem::BinaryExpr(_)
            | ValidatedBlockItem::IsExpr(_)
            | ValidatedBlockItem::UnaryExpr(_)
            | ValidatedBlockItem::CallExpr(_)
            | ValidatedBlockItem::IndexExpr(_)
            | ValidatedBlockItem::TaggedTemplateExpr(_)
            | ValidatedBlockItem::OptionalCallExpr(_)
            | ValidatedBlockItem::OptionalIndexExpr(_)
            | ValidatedBlockItem::FieldAccessExpr(_)
            | ValidatedBlockItem::UpcastExpr(_)
            | ValidatedBlockItem::QualifiedPathExpr(_)
            | ValidatedBlockItem::SpecExpr(_)
            | ValidatedBlockItem::OptionalFieldAccessExpr(_)
            | ValidatedBlockItem::EnvAccessExpr(_)
            | ValidatedBlockItem::ParenExpr(_)
            | ValidatedBlockItem::IfExpr(_)
            | ValidatedBlockItem::IfLetExpr(_)
            | ValidatedBlockItem::MatchExpr(_)
            | ValidatedBlockItem::CatchExpr(_)
            | ValidatedBlockItem::ThrowExpr(_)
            | ValidatedBlockItem::ReturnExpr(_)
            | ValidatedBlockItem::BreakExpr(_)
            | ValidatedBlockItem::ContinueExpr(_)
            | ValidatedBlockItem::SpawnExpr(_)
            | ValidatedBlockItem::AwaitExpr(_)
            | ValidatedBlockItem::LambdaExpr(_)
            | ValidatedBlockItem::ObjectLiteral(_)
            | ValidatedBlockItem::ArrayLiteral(_)
            | ValidatedBlockItem::MapLiteral(_) => self
                .cast::<raw_ast::ExprNode>()
                .expect("validated expression block item")
                .print(shape, printer),
            ValidatedBlockItem::TypeBindingStmt(_) | ValidatedBlockItem::ThrowStmt(_) => {
                let range = TextRange::new(
                    self.first_token_range().start(),
                    self.last_token_range().end(),
                );
                printer.print_input_range_trimmed_start(range);
                PrintInfo {
                    multi_lined: printer.input[range].contains('\n'),
                }
            }
            ValidatedBlockItem::DeferStmt(node) => {
                let range = TextRange::new(
                    node.first_token_range().start(),
                    node.last_token_range().end(),
                );
                printer.print_input_range_trimmed_start(range);
                PrintInfo {
                    multi_lined: printer.input[range].contains('\n'),
                }
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

fn print_binding_introducer(
    let_token: Option<ValidatedSyntaxToken>,
    const_token: Option<ValidatedSyntaxToken>,
    printer: &mut Printer,
) {
    if let Some(keyword) = let_token.or(const_token) {
        printer.print_raw_token(&keyword);
        printer.print_str(" ");
        if printer.print_trivia_squished(printer.trivia.get_for_range_split(keyword.span()).1) > 0 {
            printer.print_str(" ");
        }
    }
}

fn print_validated_for_binding(
    binding: Validated<'_, raw_ast::LetStmt>,
    shape: Shape,
    printer: &mut Printer,
) -> PrintInfo {
    print_binding_introducer(binding.let_token(), binding.const_token(), printer);
    binding.pattern().print(shape, printer)
}

fn print_validated_for_iterator(
    expression: Validated<'_, raw_ast::ForExpr>,
    shape: Shape,
    printer: &mut Printer,
) -> PrintInfo {
    let open = expression.l_paren_token();
    let close = expression.r_paren_token();
    if let Some(open) = open {
        printer.print_raw_token(&open);
    }
    let bindings = expression.let_stmt().collect::<Vec<_>>();
    let values = expression.expr_node().collect::<Vec<_>>();
    let mut info = if let Some(binding) = bindings.first().copied() {
        print_validated_for_binding(binding, shape.clone(), printer)
    } else if let Some(binding) = values.first().copied() {
        binding.print(shape.clone(), printer)
    } else {
        PrintInfo::default_single_line()
    };
    printer.print_str(" ");
    let in_token = expression
        .in_tokens()
        .next()
        .expect("validated iterator for expression");
    printer.print_raw_token(&in_token);
    printer.print_str(" ");
    let iterable = if bindings.is_empty() {
        values.get(1).copied()
    } else {
        values.first().copied()
    }
    .expect("validated iterator expression");
    info.multi_lined |= iterable.print(shape, printer).multi_lined;
    if let Some(close) = close {
        printer.print_raw_token(&close);
    }
    info
}

fn print_validated_for_c_style(
    expression: Validated<'_, raw_ast::ForExpr>,
    shape: Shape,
    printer: &mut Printer,
) -> PrintInfo {
    let open = expression
        .l_paren_token()
        .expect("validated C-style for open paren");
    let close = expression
        .r_paren_token()
        .expect("validated C-style for close paren");
    let binding = expression.let_stmt().next();
    let values = expression.expr_node().collect::<Vec<_>>();
    let mut info = PrintInfo::default_single_line();
    printer.print_raw_token(&open);
    let offset = usize::from(binding.is_none());
    if let Some(binding) = binding {
        info.multi_lined |= binding.print(shape.clone(), printer).multi_lined;
    } else if let Some(initializer) = values.first() {
        info.multi_lined |= initializer.print(shape.clone(), printer).multi_lined;
        printer.print_str(";");
    } else {
        printer.print_str(";");
    }
    printer.print_str(" ");
    if let Some(condition) = values.get(offset) {
        info.multi_lined |= condition.print(shape.clone(), printer).multi_lined;
    }
    printer.print_str(";");
    printer.print_str(" ");
    if let Some(update) = values.get(offset + 1) {
        info.multi_lined |= update.print(shape, printer).multi_lined;
    }
    printer.print_raw_token(&close);
    info
}

impl Printable for Validated<'_, raw_ast::ForExpr> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.for_token());
        printer.print_str(" ");
        let mut info = if self.in_tokens().next().is_some() {
            print_validated_for_iterator(*self, shape.clone(), printer)
        } else {
            print_validated_for_c_style(*self, shape.clone(), printer)
        };
        printer.print_str(" ");
        info.multi_lined |= self.body().print(shape, printer).multi_lined;
        info
    }

    fn leftmost_token(&self) -> TextRange {
        self.for_token().span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.body().rightmost_token()
    }
}

impl Printable for Validated<'_, raw_ast::LetStmt> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        print_binding_introducer(self.let_token(), self.const_token(), printer);
        let pattern = self.pattern();
        let mut info = pattern.print(shape.clone(), printer);
        if let Some((equals, value)) = self.equals_token().zip(self.value()) {
            printer.print_str(" ");
            printer.print_raw_token(&equals);
            printer.print_str(" ");
            printer.print_trivia_squished(printer.trivia.get_for_range_split(equals.span()).1);
            printer.print_trivia_squished(printer.trivia.get_leading_for_element(&value));
            info.multi_lined |= value.print(shape.clone(), printer).multi_lined;
            if self.else_token().is_some() || self.semicolon_token().is_some() {
                printer.print_trivia_squished(printer.trivia.get_trailing_for_element(&value));
            }
        }
        if let Some((keyword, block)) = self.else_token().zip(self.block_expr()) {
            let (leading, trailing) = printer.trivia.get_for_range_split(keyword.span());
            printer.print_str(" ");
            printer.print_trivia_squished(leading);
            printer.print_raw_token(&keyword);
            printer.print_str(" ");
            printer.print_trivia_squished(trailing);
            info.multi_lined |= block.print(shape, printer).multi_lined;
        }
        if let Some(semicolon) = self.semicolon_token() {
            printer.print_trivia_squished(printer.trivia.get_for_range_split(semicolon.span()).0);
            printer.print_raw_token(&semicolon);
        } else {
            printer.print_str(";");
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

impl Printable for Validated<'_, raw_ast::WhileStmt> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.while_token());
        printer.print_str(" ");
        self.condition().print(
            Shape {
                first_line_offset: shape.first_line_offset + "while ".len(),
                ..shape
            },
            printer,
        );
        printer.print_str(" ");
        self.body().print(shape, printer);
        PrintInfo::default_multi_lined()
    }

    fn leftmost_token(&self) -> TextRange {
        self.while_token().span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.body().rightmost_token()
    }
}

impl Printable for Validated<'_, raw_ast::WhileLetStmt> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.while_token());
        printer.print_str(" ");
        print_binding_introducer(self.let_token(), self.const_token(), printer);
        self.pattern().print(shape.clone(), printer);
        printer.print_str(" ");
        printer.print_raw_token(&self.equals_token());
        printer.print_str(" ");
        self.scrutinee().print(shape.clone(), printer);
        printer.print_str(" ");
        self.body().print(shape, printer);
        PrintInfo::default_multi_lined()
    }

    fn leftmost_token(&self) -> TextRange {
        self.while_token().span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.body().rightmost_token()
    }
}

impl Printable for Validated<'_, raw_ast::ReturnStmt> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let keyword = self.return_token();
        printer.print_raw_token(&keyword);
        if self.value().is_some() || self.semicolon_token().is_some() {
            printer.print_trivia_squished(printer.trivia.get_for_range_split(keyword.span()).1);
        }
        if let Some(value) = self.value() {
            printer.print_str(" ");
            let (leading, trailing) = printer.trivia.get_for_element(&value);
            printer.print_trivia_squished(leading);
            value.print(shape, printer);
            if self.semicolon_token().is_some() {
                printer.print_trivia_squished(trailing);
            }
        }
        if let Some(semicolon) = self.semicolon_token() {
            printer.print_trivia_squished(printer.trivia.get_for_range_split(semicolon.span()).0);
            printer.print_raw_token(&semicolon);
        } else {
            printer.print_str(";");
        }
        PrintInfo::default_single_line()
    }

    fn leftmost_token(&self) -> TextRange {
        self.return_token().span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for Validated<'_, raw_ast::BreakStmt> {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.break_token());
        if let Some(semicolon) = self.semicolon_token() {
            printer.print_raw_token(&semicolon);
        } else {
            printer.print_str(";");
        }
        PrintInfo::default_single_line()
    }

    fn leftmost_token(&self) -> TextRange {
        self.break_token().span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for Validated<'_, raw_ast::ContinueStmt> {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.continue_token());
        if let Some(semicolon) = self.semicolon_token() {
            printer.print_raw_token(&semicolon);
        } else {
            printer.print_str(";");
        }
        PrintInfo::default_single_line()
    }

    fn leftmost_token(&self) -> TextRange {
        self.continue_token().span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}
