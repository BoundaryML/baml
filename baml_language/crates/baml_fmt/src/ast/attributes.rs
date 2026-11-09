use baml_syntax::{
    nodes::{AnyAttrArg, AttrArgs, Attribute, BlockAttr, DottedName, JinjaArg, Name},
    tree::{Id, SepItem},
};

use super::{present, union_printable};
use crate::printer::{PrintInfo, PrintMultiLine, Printable, Printer, Shape, Spanned};

pub(crate) trait DottedNameExt {
    /// The parts of the name, as written.
    fn name_parts_str<'s>(self, printer: &Printer<'s>) -> Vec<&'s str>;
    /// The length of the name as printed: the parts joined by `.`.
    fn printed_len(self, printer: &Printer) -> usize;
}
impl DottedNameExt for Id<DottedName> {
    fn name_parts_str<'s>(self, printer: &Printer<'s>) -> Vec<&'s str> {
        let tree = printer.tree;
        let node = tree.get(self);
        let text =
            |name: Id<Name>| printer.input[tree.tokens().range(tree.get(name).token.idx)].as_ref();
        std::iter::once(text(node.head))
            .chain(
                tree.list(node.segments)
                    .iter()
                    .map(|segment| text(present(segment.name))),
            )
            .collect()
    }
    fn printed_len(self, printer: &Printer) -> usize {
        let parts = self.name_parts_str(printer);
        parts.iter().map(|part| part.len()).sum::<usize>() + parts.len().saturating_sub(1)
    }
}

pub(crate) trait AttributeExt {
    /// The length of the `@`, name, and `(` (if present).
    fn non_wrappable_len(self, printer: &Printer) -> usize;
}
impl AttributeExt for Id<Attribute> {
    fn non_wrappable_len(self, printer: &Printer) -> usize {
        let node = printer.tree.get(self);
        (const { "@".len() })
            + present(node.path).printed_len(printer)
            + if node.args.is_some() {
                const { "(".len() }
            } else {
                0
            }
    }
}

impl Printable for Id<BlockAttr> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        let mut multi_lined = false;
        printer.print_token(node.at);
        multi_lined |= printer.print(present(node.path), shape).multi_lined;
        if let Some(args) = node.args {
            multi_lined |= printer.print(args, shape).multi_lined;
        }
        PrintInfo { multi_lined }
    }
}

impl Printable for Id<Attribute> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        printer.print_token(node.at);
        printer.print(present(node.path), shape);
        if let Some(args) = node.args {
            printer.print(args, shape)
        } else {
            PrintInfo::default_single_line()
        }
    }
}

impl Printable for Id<DottedName> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        printer.print(node.head, shape);
        for segment in tree.list(node.segments) {
            printer.print_token(segment.dot);
            printer.print(present(segment.name), shape);
        }
        PrintInfo::default_single_line()
    }
}

impl PrintMultiLine for Id<AttrArgs> {
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

/// Should be passed a sub-printer to avoid printing trivia in the outer printer
/// in the event that the printer is unable to fit the attribute args on a single line.
fn attr_args_single_line(
    id: Id<AttrArgs>,
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    let tree = printer.tree;
    let node = tree.get(id);
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
            // Trailing comma is removed in single-line mode, but we still try the comments.
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

impl Printable for Id<AttrArgs> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| attr_args_single_line(self, &shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
}

impl Printable for Id<JinjaArg> {
    fn print(self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_verbatim(self)
    }
}

union_printable!(AnyAttrArg {
    String,
    Backtick,
    Jinja,
    Unquoted,
});
