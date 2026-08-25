use baml_db::baml_compiler_syntax::validated::{
    ValidatedToken as _,
    nodes::{
        Attribute, AttributeArg, AttributeArgs, AttributeName, AttributeNamePart, BlockAttribute,
    },
};
use rowan::{TextRange, TextSize};

use crate::printer::{PrintInfo, PrintMultiLine, Printable, Printer, Shape};

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
            Self::Word(word) => printer.print_raw_token(word),
            Self::Keyword(range) => printer.print_input_range(*range),
        }
        PrintInfo::default_single_line()
    }

    fn leftmost_token(&self) -> TextRange {
        match self {
            Self::Word(word) => word.span(),
            Self::Keyword(range) => *range,
        }
    }

    fn rightmost_token(&self) -> TextRange {
        self.leftmost_token()
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

fn try_print_single_line(
    args: &AttributeArgs,
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    printer.print_raw_token(&args.open_paren);
    let (_, open_trailing) = printer.trivia.get_for_range_split(args.open_paren.span());
    printer.try_print_trivia_single_line_squished(open_trailing)?;

    for (index, (arg, comma)) in args.args.iter().enumerate() {
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
        if index + 1 < args.args.len() {
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

    let (close_leading, _) = printer.trivia.get_for_range_split(args.close_paren.span());
    printer.try_print_trivia_single_line_squished(close_leading)?;
    printer.print_raw_token(&args.close_paren);

    (printer.output.len() <= shape.width).then(PrintInfo::default_single_line)
}

impl Printable for AttributeArgs {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|subprinter| try_print_single_line(self, &shape, subprinter))
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
            Self::QuotedString(string) => printer.print(string, shape),
            Self::RawString(string) => printer.print(string, shape),
            Self::Backtick(string) => printer.print(string, shape),
            Self::AttrExpr(range) => {
                printer.print_input_range(*range);
                PrintInfo {
                    multi_lined: printer.input[*range].contains('\n'),
                }
            }
            Self::UnquotedString(string) => {
                printer.print_raw_token(string);
                PrintInfo::default_single_line()
            }
        }
    }

    fn leftmost_token(&self) -> TextRange {
        match self {
            Self::QuotedString(string) => string.leftmost_token(),
            Self::RawString(string) => string.leftmost_token(),
            Self::Backtick(string) => string.leftmost_token(),
            Self::AttrExpr(range) => {
                TextRange::new(range.start(), range.start() + TextSize::from(1))
            }
            Self::UnquotedString(string) => string.span(),
        }
    }

    fn rightmost_token(&self) -> TextRange {
        match self {
            Self::QuotedString(string) => string.rightmost_token(),
            Self::RawString(string) => string.rightmost_token(),
            Self::Backtick(string) => string.rightmost_token(),
            Self::AttrExpr(range) => TextRange::new(range.end(), range.end() + TextSize::from(1)),
            Self::UnquotedString(string) => string.span(),
        }
    }
}
