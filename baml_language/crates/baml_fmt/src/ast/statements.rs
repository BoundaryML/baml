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
    printer::{PrintInfo, PrintMultiLine, Printable, Printer, Shape},
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
                let info = item.print(shape.clone(), printer);
                if let Some(semicolon) = semicolon {
                    let item_trailing = printer.trivia.get_trailing_for_element(item);
                    let semicolon_leading = printer.trivia.get_for_range_split(semicolon.span()).0;
                    let single_line = item_trailing
                        .iter()
                        .chain(semicolon_leading)
                        .all(|trivia| trivia.single_line_len(printer.input).is_some());
                    if single_line {
                        let has_comments = item_trailing
                            .iter()
                            .chain(semicolon_leading)
                            .any(crate::EmittableTrivia::is_comment);
                        if has_comments {
                            printer.print_str(" ");
                        }
                        printer.print_trivia_squished(item_trailing);
                        printer.print_trivia_squished(semicolon_leading);
                    } else {
                        printer.print_trivia_trailing(item_trailing);
                        printer.print_trivia_trailing(semicolon_leading);
                        printer.print_newline();
                        printer.print_spaces(shape.indent);
                    }
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

#[derive(Clone, Copy)]
enum ValidatedForBinding<'tree> {
    Let(Validated<'tree, raw_ast::LetStmt>),
    Bare(Validated<'tree, raw_ast::ExprNode>),
}

impl Printable for ValidatedForBinding<'_> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            Self::Let(binding) => {
                print_binding_introducer(binding.let_token(), binding.const_token(), printer);
                binding.pattern().print(shape, printer)
            }
            Self::Bare(binding) => binding.print(shape, printer),
        }
    }

    fn leftmost_token(&self) -> TextRange {
        match self {
            Self::Let(binding) => binding.first_token_range(),
            Self::Bare(binding) => binding.leftmost_token(),
        }
    }

    fn rightmost_token(&self) -> TextRange {
        match self {
            Self::Let(binding) => binding.pattern().rightmost_token(),
            Self::Bare(binding) => binding.rightmost_token(),
        }
    }
}

struct ValidatedForIteratorArgs<'tree> {
    open: Option<ValidatedSyntaxToken>,
    binding: ValidatedForBinding<'tree>,
    in_keyword: ValidatedSyntaxToken,
    expression: Validated<'tree, raw_ast::ExprNode>,
    close: Option<ValidatedSyntaxToken>,
}

impl<'tree> ValidatedForIteratorArgs<'tree> {
    fn new(expression: Validated<'tree, raw_ast::ForExpr>) -> Self {
        let binding = expression.let_stmt().next().map_or_else(
            || {
                ValidatedForBinding::Bare(
                    expression
                        .expr_node()
                        .next()
                        .expect("validated iterator binding"),
                )
            },
            ValidatedForBinding::Let,
        );
        let iterable = match binding {
            ValidatedForBinding::Let(_) => expression.expr_node().next(),
            ValidatedForBinding::Bare(_) => expression.expr_node().nth(1),
        }
        .expect("validated iterator expression");
        Self {
            open: expression.l_paren_token(),
            binding,
            in_keyword: expression
                .in_tokens()
                .next()
                .expect("validated iterator keyword"),
            expression: iterable,
            close: expression.r_paren_token(),
        }
    }

    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        if let Some(open) = self.open {
            printer.print_raw_token(&open);
            printer.try_print_trivia_single_line_squished(
                printer.trivia.get_for_range_split(open.span()).1,
            )?;
        }
        let (binding_leading, binding_trailing) = printer.trivia.get_for_element(&self.binding);
        printer.try_print_trivia_single_line_squished(binding_leading)?;
        if self
            .binding
            .print(Shape::unlimited_single_line(), printer)
            .multi_lined
        {
            return None;
        }
        printer.try_print_trivia_single_line_squished(binding_trailing)?;
        printer.print_str(" ");
        printer.print_raw_token(&self.in_keyword);
        printer.print_str(" ");
        printer.print_trivia_squished(printer.trivia.get_for_range_split(self.in_keyword.span()).1);
        let (expression_leading, expression_trailing) =
            printer.trivia.get_for_element(&self.expression);
        printer.print_trivia_squished(expression_leading);
        if self
            .expression
            .print(Shape::unlimited_single_line(), printer)
            .multi_lined
        {
            return None;
        }
        printer.try_print_trivia_single_line_squished(expression_trailing)?;
        if let Some(close) = self.close {
            printer.try_print_trivia_single_line_squished(
                printer.trivia.get_for_range_split(close.span()).0,
            )?;
            printer.print_raw_token(&close);
        }
        (printer.output.len() <= shape.width).then(PrintInfo::default_single_line)
    }
}

impl PrintMultiLine for ValidatedForIteratorArgs<'_> {
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_shape =
            Shape::standalone(shape.width, shape.indent + printer.config.indent_width);
        if let Some(open) = self.open {
            printer.print_raw_token(&open);
            printer.print_trivia_all_trailing_for(open.span());
            printer.print_newline();
            printer.print_trivia_with_newline(
                printer.trivia.get_leading_for_element(&self.binding),
                inner_shape.indent,
            );
            printer.print_spaces(inner_shape.indent);
        }
        self.binding.print(inner_shape.clone(), printer);
        printer.print_str(" ");
        printer.print_raw_token(&self.in_keyword);
        printer.print_str(" ");
        printer.print_trivia_squished(printer.trivia.get_for_range_split(self.in_keyword.span()).1);
        let (expression_leading, expression_trailing) =
            printer.trivia.get_for_element(&self.expression);
        printer.print_trivia_squished(expression_leading);
        let current = printer.current_line_len();
        self.expression.print(
            Shape {
                width: printer.config.line_width.saturating_sub(current),
                indent: inner_shape.indent,
                first_line_offset: current.saturating_sub(inner_shape.indent),
            },
            printer,
        );
        printer.print_trivia_trailing(expression_trailing);
        if let Some(close) = self.close {
            printer.print_newline();
            printer.print_trivia_with_newline(
                printer.trivia.get_for_range_split(close.span()).0,
                inner_shape.indent,
            );
            printer.print_spaces(shape.indent);
            printer.print_raw_token(&close);
        }
        PrintInfo::default_multi_lined()
    }
}

impl Printable for ValidatedForIteratorArgs<'_> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|probe| self.try_print_single_line(&shape, probe))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }

    fn leftmost_token(&self) -> TextRange {
        self.open
            .map_or_else(|| self.binding.leftmost_token(), |token| token.span())
    }

    fn rightmost_token(&self) -> TextRange {
        self.close
            .map_or_else(|| self.expression.rightmost_token(), |token| token.span())
    }
}

#[derive(Clone, Copy)]
enum ValidatedForInitializer<'tree> {
    Let(Validated<'tree, raw_ast::LetStmt>),
    Expr(Validated<'tree, raw_ast::ExprNode>, ValidatedSyntaxToken),
    Empty(ValidatedSyntaxToken),
}

impl Printable for ValidatedForInitializer<'_> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            Self::Let(binding) => binding.print(shape, printer),
            Self::Expr(value, semicolon) => {
                let info = value.print(shape, printer);
                printer.print_trivia_squished(printer.trivia.get_trailing_for_element(value));
                let (leading, _) = printer.trivia.get_for_range_split(semicolon.span());
                printer.print_trivia_squished(leading);
                printer.print_raw_token(semicolon);
                info
            }
            Self::Empty(semicolon) => {
                printer.print_raw_token(semicolon);
                PrintInfo::default_single_line()
            }
        }
    }

    fn leftmost_token(&self) -> TextRange {
        match self {
            Self::Let(binding) => binding.first_token_range(),
            Self::Expr(value, _) => value.leftmost_token(),
            Self::Empty(semicolon) => semicolon.span(),
        }
    }

    fn rightmost_token(&self) -> TextRange {
        match self {
            Self::Let(binding) => binding.last_token_range(),
            Self::Expr(_, semicolon) => semicolon.span(),
            Self::Empty(semicolon) => semicolon.span(),
        }
    }
}

struct ValidatedForCStyleArgs<'tree> {
    open: ValidatedSyntaxToken,
    init: ValidatedForInitializer<'tree>,
    condition: Option<Validated<'tree, raw_ast::ExprNode>>,
    semicolon: ValidatedSyntaxToken,
    update: Option<Validated<'tree, raw_ast::ExprNode>>,
    close: ValidatedSyntaxToken,
}

impl<'tree> ValidatedForCStyleArgs<'tree> {
    fn new(expression: Validated<'tree, raw_ast::ForExpr>) -> Option<Self> {
        enum Stage {
            Init,
            Condition,
            Update,
        }

        let open = expression.l_paren_token()?;
        let close = expression.r_paren_token()?;
        let mut stage = Stage::Init;
        let mut binding = None;
        let mut init_value = None;
        let mut init = None;
        let mut condition = None;
        let mut semicolon = None;
        let mut update = None;

        for element in expression.direct_elements() {
            if element.kind() == SyntaxKind::R_PAREN {
                break;
            }
            if let Some(value) = element.node::<raw_ast::LetStmt>() {
                if !matches!(stage, Stage::Init) || binding.replace(value).is_some() {
                    return None;
                }
                stage = Stage::Condition;
            } else if let Some(value) = element.node::<raw_ast::ExprNode>() {
                let slot = match stage {
                    Stage::Init => &mut init_value,
                    Stage::Condition => &mut condition,
                    Stage::Update => &mut update,
                };
                if slot.replace(value).is_some() {
                    return None;
                }
            } else if let Some(token) = element.token()
                && token.kind() == SyntaxKind::SEMICOLON
            {
                match stage {
                    Stage::Init => {
                        init = Some(
                            init_value.map_or(ValidatedForInitializer::Empty(token), |value| {
                                ValidatedForInitializer::Expr(value, token)
                            }),
                        );
                        stage = Stage::Condition;
                    }
                    Stage::Condition => {
                        if semicolon.replace(token).is_some() {
                            return None;
                        }
                        stage = Stage::Update;
                    }
                    Stage::Update => return None,
                }
            }
        }

        let init = match (binding, init) {
            (Some(binding), None) => ValidatedForInitializer::Let(binding),
            (None, Some(init)) => init,
            _ => return None,
        };

        Some(Self {
            open,
            init,
            condition,
            semicolon: semicolon?,
            update,
            close,
        })
    }

    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.open);
        printer.try_print_trivia_single_line_squished(
            printer.trivia.get_for_range_split(self.open.span()).1,
        )?;
        let (init_leading, init_trailing) = printer.trivia.get_for_element(&self.init);
        printer.try_print_trivia_single_line_squished(init_leading)?;
        if self
            .init
            .print(Shape::unlimited_single_line(), printer)
            .multi_lined
        {
            return None;
        }
        printer.try_print_trivia_single_line_squished(init_trailing)?;
        if let Some(condition) = self.condition {
            printer.print_str(" ");
            let (condition_leading, condition_trailing) =
                printer.trivia.get_for_element(&condition);
            printer.try_print_trivia_single_line_squished(condition_leading)?;
            if condition
                .print(Shape::unlimited_single_line(), printer)
                .multi_lined
            {
                return None;
            }
            printer.print_trivia_squished(condition_trailing);
        }
        let (semicolon_leading, semicolon_trailing) =
            printer.trivia.get_for_range_split(self.semicolon.span());
        printer.print_trivia_squished(semicolon_leading);
        printer.print_raw_token(&self.semicolon);
        printer.try_print_trivia_single_line_squished(semicolon_trailing)?;
        if let Some(update) = self.update {
            printer.print_str(" ");
            let (update_leading, update_trailing) = printer.trivia.get_for_element(&update);
            printer.try_print_trivia_single_line_squished(update_leading)?;
            if update
                .print(Shape::unlimited_single_line(), printer)
                .multi_lined
            {
                return None;
            }
            printer.try_print_trivia_single_line_squished(update_trailing)?;
        }
        printer.try_print_trivia_single_line_squished(
            printer.trivia.get_for_range_split(self.close.span()).0,
        )?;
        printer.print_raw_token(&self.close);
        (printer.output.len() <= shape.width).then(PrintInfo::default_single_line)
    }
}

impl PrintMultiLine for ValidatedForCStyleArgs<'_> {
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_shape = Shape::standalone(
            printer.config.line_width,
            shape.indent + printer.config.indent_width,
        );
        printer.print_raw_token(&self.open);
        printer.print_trivia_all_trailing_for(self.open.span());
        printer.print_newline();
        let (init_leading, init_trailing) = printer.trivia.get_for_element(&self.init);
        printer.print_trivia_with_newline(init_leading.trim_blanks(), inner_shape.indent);
        printer.print_spaces(inner_shape.indent);
        self.init.print(inner_shape.clone(), printer);
        printer.print_trivia_trailing(init_trailing);
        printer.print_newline();
        if let Some(condition) = self.condition {
            let (condition_leading, condition_trailing) =
                printer.trivia.get_for_element(&condition);
            printer.print_trivia_with_newline(condition_leading.trim_blanks(), inner_shape.indent);
            printer.print_spaces(inner_shape.indent);
            condition.print(inner_shape.clone(), printer);
            printer.print_trivia_squished(condition_trailing);
        } else {
            printer.print_spaces(inner_shape.indent);
        }
        let (semicolon_leading, semicolon_trailing) =
            printer.trivia.get_for_range_split(self.semicolon.span());
        printer.print_trivia_squished(semicolon_leading);
        printer.print_raw_token(&self.semicolon);
        printer.print_trivia_trailing(semicolon_trailing);
        if let Some(update) = self.update {
            printer.print_newline();
            let (update_leading, update_trailing) = printer.trivia.get_for_element(&update);
            printer.print_trivia_with_newline(update_leading.trim_blanks(), inner_shape.indent);
            printer.print_spaces(inner_shape.indent);
            update.print(inner_shape.clone(), printer);
            printer.print_trivia_trailing(update_trailing);
        }
        printer.print_newline();
        printer.print_trivia_with_newline(
            printer
                .trivia
                .get_for_range_split(self.close.span())
                .0
                .trim_blanks(),
            inner_shape.indent,
        );
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close);
        PrintInfo::default_multi_lined()
    }
}

impl Printable for ValidatedForCStyleArgs<'_> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|probe| self.try_print_single_line(&shape, probe))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }

    fn leftmost_token(&self) -> TextRange {
        self.open.span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.close.span()
    }
}

impl Printable for Validated<'_, raw_ast::ForExpr> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.for_token());
        printer.print_str(" ");
        let mut info = if self.in_tokens().next().is_some() {
            ValidatedForIteratorArgs::new(*self).print(shape.clone(), printer)
        } else if let Some(args) = ValidatedForCStyleArgs::new(*self) {
            args.print(shape.clone(), printer)
        } else {
            let body = self.body();
            let range = TextRange::new(
                self.for_token().span().end(),
                body.first_token_range().start(),
            );
            printer.print_input_range_trimmed_start(range);
            PrintInfo {
                multi_lined: printer.input[range].contains('\n'),
            }
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
