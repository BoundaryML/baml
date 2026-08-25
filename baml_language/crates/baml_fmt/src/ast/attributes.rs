use baml_db::baml_compiler_syntax::{
    FromCST, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken, ast as syntax_ast,
    validated::Validated,
};
use rowan::{TextRange, ast::AstNode as _};

use crate::{
    ast::Token as _,
    printer::{PrintInfo, PrintMultiLine, Printable, Printer, Shape},
};

#[derive(Clone)]
struct RawToken(SyntaxToken);

impl crate::ast::Token for RawToken {
    fn span(&self) -> TextRange {
        self.0.text_range()
    }
}

fn non_trivia_range(node: &SyntaxNode) -> TextRange {
    let mut tokens = node
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|token| !token.kind().is_trivia());
    let first = tokens.next().expect("validated attribute node");
    let last = tokens.last().unwrap_or_else(|| first.clone());
    TextRange::new(first.text_range().start(), last.text_range().end())
}

fn first_non_trivia_range(node: &SyntaxNode) -> TextRange {
    node.descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|token| !token.kind().is_trivia())
        .expect("validated syntax node")
        .text_range()
}

fn last_non_trivia_range(node: &SyntaxNode) -> TextRange {
    node.descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|token| !token.kind().is_trivia())
        .last()
        .expect("validated syntax node")
        .text_range()
}

fn print_raw_attribute(
    syntax: &SyntaxNode,
    args: Option<&syntax_ast::AttributeArgs>,
    shape: &Shape,
    printer: &mut Printer,
) -> PrintInfo {
    let mut multi_lined = false;
    for element in syntax
        .children_with_tokens()
        .filter(|element| !element.kind().is_trivia())
    {
        match element {
            rowan::NodeOrToken::Token(token) => printer.print_raw_token(&RawToken(token)),
            rowan::NodeOrToken::Node(node) => {
                if let Some(attribute_args) = args.filter(|args| args.syntax() == &node) {
                    multi_lined |= printer.print(attribute_args, (*shape).clone()).multi_lined;
                }
            }
        }
    }
    PrintInfo { multi_lined }
}

impl Printable for syntax_ast::BlockAttribute {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        print_raw_attribute(
            self.syntax(),
            self.attribute_args().as_ref(),
            &shape,
            printer,
        )
    }

    fn leftmost_token(&self) -> TextRange {
        first_non_trivia_range(self.syntax())
    }

    fn rightmost_token(&self) -> TextRange {
        last_non_trivia_range(self.syntax())
    }
}

impl Printable for syntax_ast::Attribute {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        print_raw_attribute(
            self.syntax(),
            self.attribute_args().as_ref(),
            &shape,
            printer,
        )
    }

    fn leftmost_token(&self) -> TextRange {
        first_non_trivia_range(self.syntax())
    }

    fn rightmost_token(&self) -> TextRange {
        last_non_trivia_range(self.syntax())
    }
}

impl Printable for Validated<'_, syntax_ast::BlockAttribute> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        syntax_ast::BlockAttribute::cast(self.syntax().clone())
            .expect("validated block attribute")
            .print(shape, printer)
    }

    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for Validated<'_, syntax_ast::Attribute> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        syntax_ast::Attribute::cast(self.syntax().clone())
            .expect("validated attribute")
            .print(shape, printer)
    }

    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

#[derive(Clone)]
struct RawAttributeArg(SyntaxNode);

impl Printable for RawAttributeArg {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self.0.kind() {
            SyntaxKind::STRING_LITERAL => printer.print(
                &crate::ast::tokens::QuotedString::from_cst(SyntaxElement::Node(self.0.clone()))
                    .expect("validated quoted attribute argument"),
                shape,
            ),
            SyntaxKind::RAW_STRING_LITERAL => printer.print(
                &crate::ast::tokens::RawString::from_cst(SyntaxElement::Node(self.0.clone()))
                    .expect("validated raw attribute argument"),
                shape,
            ),
            SyntaxKind::BACKTICK_STRING_LITERAL => printer.print(
                &crate::ast::tokens::BacktickString::from_cst(SyntaxElement::Node(self.0.clone()))
                    .expect("validated backtick attribute argument"),
                shape,
            ),
            _ => {
                let range = non_trivia_range(&self.0);
                printer.print_input_range(range);
                PrintInfo {
                    multi_lined: printer.input[range].contains('\n'),
                }
            }
        }
    }

    fn leftmost_token(&self) -> TextRange {
        first_non_trivia_range(&self.0)
    }

    fn rightmost_token(&self) -> TextRange {
        last_non_trivia_range(&self.0)
    }
}

fn raw_attribute_args(
    args: &syntax_ast::AttributeArgs,
) -> Vec<(RawAttributeArg, Option<RawToken>)> {
    let mut elements = args
        .syntax()
        .children_with_tokens()
        .filter(|element| !element.kind().is_trivia())
        .peekable();
    let mut result = Vec::new();
    while let Some(element) = elements.next() {
        let Some(node) = element.into_node() else {
            continue;
        };
        let delimiter = elements
            .next_if(|element| matches!(element.kind(), SyntaxKind::COMMA | SyntaxKind::SEMICOLON))
            .and_then(rowan::NodeOrToken::into_token)
            .map(RawToken);
        result.push((RawAttributeArg(node), delimiter));
    }
    result
}

impl PrintMultiLine for syntax_ast::AttributeArgs {
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_indent = shape.indent + printer.config.indent_width;
        let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
        let open = RawToken(self.l_paren_token().expect("validated attribute arguments"));
        let close = RawToken(self.r_paren_token().expect("validated attribute arguments"));
        printer.print_raw_token(&open);
        printer.print_trivia_all_trailing_for(open.span());
        printer.print_newline();
        for (argument, comma) in raw_attribute_args(self) {
            printer
                .print_trivia_all_leading_with_newline_for(argument.leftmost_token(), inner_indent);
            printer.print_spaces(inner_indent);
            printer.print(&argument, inner_shape.clone());
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
}

impl Printable for syntax_ast::AttributeArgs {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let arguments = raw_attribute_args(self);
        printer
            .try_sub_printer(|subprinter| {
                let open = RawToken(self.l_paren_token()?);
                let close = RawToken(self.r_paren_token()?);
                subprinter.print_raw_token(&open);
                subprinter.try_print_trivia_single_line_squished(
                    subprinter.trivia.get_for_range_split(open.span()).1,
                )?;
                for (index, (argument, comma)) in arguments.iter().enumerate() {
                    let (leading, trailing) = subprinter.trivia.get_for_element(argument);
                    subprinter.try_print_trivia_single_line_squished(leading)?;
                    if subprinter
                        .print(argument, Shape::unlimited_single_line())
                        .multi_lined
                    {
                        return None;
                    }
                    subprinter.try_print_trivia_single_line_squished(trailing)?;
                    if index + 1 < arguments.len() {
                        if let Some(comma) = comma {
                            let (leading, trailing) =
                                subprinter.trivia.get_for_range_split(comma.span());
                            subprinter.try_print_trivia_single_line_squished(leading)?;
                            subprinter.print_raw_token(comma);
                            subprinter.try_print_trivia_single_line_squished(trailing)?;
                        } else {
                            subprinter.print_str(",");
                        }
                        subprinter.print_str(" ");
                    } else if let Some(comma) = comma {
                        let (leading, trailing) =
                            subprinter.trivia.get_for_range_split(comma.span());
                        subprinter.try_print_trivia_single_line_squished(leading)?;
                        subprinter.try_print_trivia_single_line_squished(trailing)?;
                    }
                }
                subprinter.try_print_trivia_single_line_squished(
                    subprinter.trivia.get_for_range_split(close.span()).0,
                )?;
                subprinter.print_raw_token(&close);
                (subprinter.output.len() <= shape.width).then(PrintInfo::default_single_line)
            })
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }

    fn leftmost_token(&self) -> TextRange {
        self.l_paren_token()
            .expect("validated attribute arguments")
            .text_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.r_paren_token()
            .expect("validated attribute arguments")
            .text_range()
    }
}
