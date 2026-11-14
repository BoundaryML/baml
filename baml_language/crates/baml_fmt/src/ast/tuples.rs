use baml_syntax::{
    nodes::{TupleExpr, TuplePattern, TupleType},
    tree::Id,
};

use super::present;
use crate::{
    printer::{PrintInfo, Printable, Printer, Shape},
    trivia_classifier::TriviaSliceExt as _,
};

macro_rules! tuple_print {
    ($ty:ty) => {
        impl Printable for Id<$ty> {
            fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
                let tree = printer.tree;
                let node = tree.get(self);
                let elements = tree.list(node.elements);
                let close = present(node.r);
                let single = printer.try_sub_printer(|p| {
                    p.print_token(node.l);
                    p.try_print_trivia_single_line_squished(
                        p.trivia.get_for_range_split(p.range(node.l)).1,
                    )?;
                    for (index, element) in elements.iter().enumerate() {
                        let (leading, trailing) = p.trivia.get_for_element(tree, element.item);
                        p.try_print_trivia_single_line_squished(leading)?;
                        if p.print(element.item, Shape::unlimited_single_line())
                            .multi_lined
                        {
                            return None;
                        }
                        p.try_print_trivia_single_line_squished(trailing)?;
                        if index + 1 < elements.len() || elements.len() == 1 {
                            if let Some(comma) = element.sep {
                                p.print_token(comma);
                            } else {
                                p.print_str(",");
                            }
                            if index + 1 < elements.len() {
                                p.print_str(" ");
                            }
                        }
                        if let Some(comma) = element.sep {
                            p.try_print_trivia_single_line_squished(
                                p.trivia.get_for_range_split(p.range(comma)).1,
                            )?;
                        }
                    }
                    p.try_print_trivia_single_line_squished(
                        p.trivia.get_for_range_split(p.range(close)).0,
                    )?;
                    p.print_token(close);
                    (p.output.len() <= shape.width).then_some(PrintInfo::default_single_line())
                });
                if let Some(info) = single {
                    return info;
                }
                printer.print_token(node.l);
                printer.print_trivia_all_trailing_for(printer.range(node.l));
                printer.print_newline();
                let indent = shape.indent + printer.config.indent_width;
                for element in elements {
                    let (leading, trailing) = printer.trivia.get_for_element(tree, element.item);
                    printer.print_trivia_with_newline(leading.trim_blanks(), indent);
                    printer.print_spaces(indent);
                    printer.print(
                        element.item,
                        Shape::standalone(printer.config.line_width, indent),
                    );
                    printer.print_trivia_squished(trailing);
                    if let Some(comma) = element.sep {
                        printer.print_token(comma);
                        printer.print_trivia_all_trailing_for(printer.range(comma));
                    } else {
                        printer.print_str(",");
                    }
                    printer.print_newline();
                }
                printer
                    .print_trivia_all_leading_with_newline_for(printer.range(close), shape.indent);
                printer.print_spaces(shape.indent);
                printer.print_token(close);
                PrintInfo::default_multi_lined()
            }
        }
    };
}
tuple_print!(TupleExpr);
tuple_print!(TupleType);
tuple_print!(TuplePattern);
