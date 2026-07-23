pub use baml_db::baml_compiler_syntax::{
    AttributeArg, AttributeName, AttributeNamePart, ValidatedAttribute as Attribute,
    ValidatedAttributeArgs as AttributeArgs, ValidatedBlockAttribute as BlockAttribute,
};
use rowan::{TextRange, TextSize};

use crate::{
    ast::Token,
    printer::{PrintInfo, PrintMultiLine, Printable, Printer, Shape},
};

impl Printable for BlockAttribute {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        printer.print_raw_token(&self.atat);
        multi_lined |= printer.print(&self.name, shape.clone()).multi_lined;
        if let Some(args) = &self.args {
            multi_lined |= printer.print(args, shape).multi_lined;
        }
        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        self.atat.span()
    }
    fn rightmost_token(&self) -> TextRange {
        if let Some(args) = &self.args {
            args.rightmost_token()
        } else {
            self.name.rightmost_token()
        }
    }
}

impl Printable for Attribute {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.at);
        printer.print(&self.name, shape.clone());
        if let Some(args) = &self.args {
            printer.print(args, shape)
        } else {
            PrintInfo::default_single_line()
        }
    }
    fn leftmost_token(&self) -> TextRange {
        self.at.span()
    }
    fn rightmost_token(&self) -> TextRange {
        if let Some(args) = &self.args {
            args.rightmost_token()
        } else {
            self.name.rightmost_token()
        }
    }
}

impl Printable for AttributeNamePart {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            AttributeNamePart::Word(word) => printer.print_raw_token(word),
            AttributeNamePart::Keyword(range) => printer.print_input_range(*range),
        }
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            AttributeNamePart::Word(word) => word.span(),
            AttributeNamePart::Keyword(range) => *range,
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            AttributeNamePart::Word(word) => word.span(),
            AttributeNamePart::Keyword(range) => *range,
        }
    }
}

impl Printable for AttributeName {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print(&self.first, shape.clone());
        for (dot, part) in &self.rest {
            printer.print_raw_token(dot);
            printer.print(part, shape.clone());
        }
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.first.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.rest
            .last()
            .map_or(&self.first, |(_, part)| part)
            .rightmost_token()
    }
}

impl PrintMultiLine for AttributeArgs {
    /// Multi-line layout: each argument on its own indented line with trailing comma.
    /// Closing paren on its own line.
    ///
    /// ```baml
    /// (
    ///     "quoted string",
    ///     {{ this > 0 }},
    ///     #"raw string"#,
    /// )
    /// ```
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

trait AttributeArgsPrintExt {
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}

impl AttributeArgsPrintExt for AttributeArgs {
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the attribute args on a single line.
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
}

impl Printable for AttributeArgs {
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

impl Printable for AttributeArg {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            AttributeArg::QuotedString(s) => printer.print(s, shape),
            AttributeArg::RawString(s) => printer.print(s, shape),
            AttributeArg::Backtick(s) => printer.print(s, shape),
            AttributeArg::AttrExpr(range) => {
                printer.print_input_range(*range);
                PrintInfo {
                    multi_lined: printer.input[*range].contains('\n'),
                }
            }
            AttributeArg::UnquotedString(s) => {
                printer.print_raw_token(s);
                PrintInfo::default_single_line()
            }
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            AttributeArg::QuotedString(s) => s.leftmost_token(),
            AttributeArg::RawString(s) => s.leftmost_token(),
            AttributeArg::Backtick(s) => s.leftmost_token(),
            AttributeArg::AttrExpr(range) => {
                TextRange::new(range.start(), range.start() + TextSize::from(1))
            }
            AttributeArg::UnquotedString(s) => s.span(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            AttributeArg::QuotedString(s) => s.rightmost_token(),
            AttributeArg::RawString(s) => s.rightmost_token(),
            AttributeArg::Backtick(s) => s.rightmost_token(),
            AttributeArg::AttrExpr(range) => {
                TextRange::new(range.end(), range.end() + TextSize::from(1))
            }
            AttributeArg::UnquotedString(s) => s.span(),
        }
    }
}
