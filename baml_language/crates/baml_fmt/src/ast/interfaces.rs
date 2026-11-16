use baml_syntax::{
    nodes::{InterfaceDef, RequiresClause},
    tree::{Id, SepItem},
};

use super::{
    declarations::{print_declaration_body, print_docs},
    present,
};
use crate::printer::{PrintInfo, Printable, Printer, Shape, Spanned};

impl Printable for Id<InterfaceDef> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let name = present(node.name);
        let open = present(node.l);
        let close = present(node.r);
        print_docs(node.docs, shape.indent, printer);
        let continuation = shape.indent + printer.config.indent_width;
        printer.print_token(node.kw);
        printer.print_separator(
            printer.range(node.kw),
            Some(name.leftmost_token(tree)),
            continuation,
            " ",
        );
        printer.print(name, shape);
        let mut previous = name.rightmost_token(tree);
        if let Some(params) = node.generics {
            printer.print_separator(
                previous,
                Some(params.leftmost_token(tree)),
                continuation,
                "",
            );
            printer.print(params, shape);
            previous = params.rightmost_token(tree);
        }
        if let Some(requires) = node.requires {
            printer.print_separator(
                previous,
                Some(requires.leftmost_token(tree)),
                continuation,
                " ",
            );
            printer.print(requires, shape);
            previous = requires.rightmost_token(tree);
        }
        printer.print_separator(previous, Some(printer.range(open)), shape.indent, " ");
        let members = tree.list(node.members);
        if members.is_empty() && printer.try_print_empty_braces(open, close) {
            return PrintInfo::default_single_line();
        }
        print_declaration_body(open, members, close, &shape, printer)
    }
}

impl Printable for Id<RequiresClause> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let continuation = shape.indent + printer.config.indent_width;
        printer.print_token(node.kw);
        let mut previous = printer.range(node.kw);
        let mut multi_lined = false;
        for SepItem { item: ty, sep } in tree.list(present(node.types).as_list()) {
            printer.print_separator(previous, Some(ty.leftmost_token(tree)), continuation, " ");
            let mut candidate = printer.sub_printer();
            let info = candidate.print(*ty, Shape::unlimited_single_line());
            if info.multi_lined
                || printer.current_line_len() + candidate.output.len() + 2
                    > printer.config.line_width
            {
                printer
                    .output
                    .truncate(printer.output.trim_end_matches(' ').len());
                printer.print_newline();
                printer.print_spaces(continuation);
                printer.print(
                    *ty,
                    Shape::standalone(printer.config.line_width, continuation),
                );
                multi_lined = true;
            } else {
                printer.append_from_printer(candidate);
            }
            previous = ty.rightmost_token(tree);
            if let Some(comma) = *sep {
                printer.print_separator(previous, Some(printer.range(comma)), continuation, "");
                printer.print_token(comma);
                previous = printer.range(comma);
            }
        }
        PrintInfo { multi_lined }
    }
}
