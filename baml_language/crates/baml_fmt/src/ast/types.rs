//! Type expressions.

use baml_syntax::{
    nodes::{
        AnyLiteralTypeValue, AnyType, AnyTypeArg, ArrayType, AssocTypeBinding, FileTree,
        FunctionType, FunctionTypeParam, LiteralType, OptionalType, ParenType, PathType,
        QualifiedPath, ThrowsClause, TypeArgs, UnionType,
    },
    token::{Token, punct},
    tree::{Id, SepItem},
};

use super::{present, union_printable};
use crate::{
    printer::{PrintInfo, PrintMultiLine, Printable, Printer, Shape, Spanned},
    trivia_classifier::TriviaSliceExt as _,
};

union_printable!(AnyType {
    Path,
    Literal,
    Paren,
    Function,
    Optional,
    Array,
    Union,
    Qualified,
});

union_printable!(AnyTypeArg { Type, Binding });

/// Whether the type's multi-line layout indents its continuation lines.
pub(crate) fn multi_line_is_indented(tree: &FileTree, ty: AnyType) -> bool {
    match ty {
        AnyType::Path(path) => tree.get(path).args.is_none(),
        AnyType::Union(_) | AnyType::Function(_) => true,
        AnyType::Optional(optional) => multi_line_is_indented(tree, tree.get(optional).inner),
        AnyType::Array(array) => multi_line_is_indented(tree, tree.get(array).inner),
        AnyType::Paren(_) | AnyType::Literal(_) | AnyType::Qualified(_) => false,
        AnyType::Bogus(_) => unreachable!("the formatter prints only error-free trees"),
    }
}

impl Printable for Id<PathType> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        printer.print(node.head, shape);
        for segment in tree.list(node.segments) {
            printer.print_token(segment.dot);
            printer.print(present(segment.name), shape);
        }
        let multi_lined = node
            .args
            .is_some_and(|args| printer.print(args, shape).multi_lined);
        PrintInfo { multi_lined }
    }
}

impl Printable for Id<LiteralType> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        match node.value {
            // The sign and the literal are printed as written.
            AnyLiteralTypeValue::Scalar(_) => printer.print_verbatim(self),
            AnyLiteralTypeValue::Backtick(backtick) => {
                if let Some(minus) = node.minus {
                    printer.print_token(minus);
                }
                printer.print(backtick, shape)
            }
            AnyLiteralTypeValue::Bogus(_) => {
                unreachable!("the formatter prints only error-free trees")
            }
        }
    }
}

impl PrintMultiLine for Id<ParenType> {
    fn print_multi_line(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        let inner = present(node.inner);
        let close = present(node.r);
        let inner_indent = shape.indent + printer.config.indent_width;
        printer.print_token(node.l);
        printer.print_trivia_all_trailing_for(printer.range(node.l));
        printer.print_newline();
        printer.print_standalone_with_trivia(inner, inner_indent);
        printer.print_newline();
        let (close_leading, _) = printer.trivia.get_for_range_split(printer.range(close));
        printer.print_trivia_with_newline(close_leading.trim_blanks(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_token(close);
        PrintInfo::default_multi_lined()
    }
}

fn paren_type_single_line(
    id: Id<ParenType>,
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    let node = printer.tree.get(id);
    let inner = present(node.inner);
    let close = present(node.r);
    printer.print_token(node.l);
    let (_, open_trailing) = printer.trivia.get_for_range_split(printer.range(node.l));
    printer.try_print_trivia_single_line_squished(open_trailing)?;
    let (ty_leading, ty_trailing) = printer.trivia.get_for_element(printer.tree, inner);
    printer.try_print_trivia_single_line_squished(ty_leading)?;
    if printer
        .print(inner, Shape::unlimited_single_line())
        .multi_lined
    {
        return None;
    }
    printer.try_print_trivia_single_line_squished(ty_trailing)?;
    let (close_leading, _) = printer.trivia.get_for_range_split(printer.range(close));
    printer.try_print_trivia_single_line_squished(close_leading)?;
    printer.print_token(close);
    if printer.output.len() > shape.width {
        None
    } else {
        Some(PrintInfo::default_single_line())
    }
}

impl Printable for Id<ParenType> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| paren_type_single_line(self, &shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
}

impl PrintMultiLine for Id<UnionType> {
    fn print_multi_line(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let arms = tree.list(node.arms.as_list());
        let mut info = printer.print(node.first, shape);
        printer.print_trivia_all_trailing_for(node.first.rightmost_token(tree));
        let inner_indent = shape.indent + printer.config.indent_width;
        let inner_shape = Shape {
            width: printer.config.line_width.saturating_sub(inner_indent),
            indent: inner_indent,
            first_line_offset: 0,
        };
        for (i, arm) in arms.iter().enumerate() {
            let ty = present(arm.ty);
            info.multi_lined = true;
            let (pipe_leading, pipe_trailing) =
                printer.trivia.get_for_range_split(printer.range(arm.pipe));
            let (ty_leading, ty_trailing) = printer.trivia.get_for_element(tree, ty);
            printer.print_newline();
            printer.print_trivia_with_newline(pipe_leading.trim_blanks(), inner_shape.indent);
            printer.print_spaces(inner_indent);
            printer.print_token(arm.pipe);
            let mut post_pipe_len = printer.print_trivia_squished(pipe_trailing);
            post_pipe_len += printer.print_trivia_squished(ty_leading);
            if post_pipe_len == 0 {
                printer.print_spaces(1); // only add space if there are no block comments between
                post_pipe_len = 1;
            }
            let offset = const { "| ".len() } + post_pipe_len;
            let ty_shape = Shape {
                width: printer
                    .config
                    .line_width
                    .saturating_sub(inner_indent + offset),
                indent: inner_indent,
                first_line_offset: offset,
            };
            printer.print(ty, ty_shape);
            if i + 1 < arms.len() {
                printer.print_trivia_trailing(ty_trailing);
            }
        }
        info
    }
}

fn union_type_single_line(
    id: Id<UnionType>,
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    let tree = printer.tree;
    let node = tree.get(id);
    let arms = tree.list(node.arms.as_list());
    if printer
        .print(node.first, Shape::unlimited_single_line())
        .multi_lined
    {
        return None;
    }
    let first_trailing = printer.trivia.get_trailing_for_element(tree, node.first);
    let mut pre_pipe_len = printer.try_print_trivia_single_line_squished(first_trailing)?;
    for (i, arm) in arms.iter().enumerate() {
        let ty = present(arm.ty);
        if printer.output.len() > shape.width {
            return None;
        }
        let (pipe_leading, pipe_trailing) =
            printer.trivia.get_for_range_split(printer.range(arm.pipe));
        let (ty_leading, ty_trailing) = printer.trivia.get_for_element(tree, ty);
        pre_pipe_len += printer.print_trivia_squished(pipe_leading);
        if pre_pipe_len == 0 {
            printer.print_spaces(1); // only add space if there are no block comments between
        }
        printer.print_token(arm.pipe);
        let mut post_pipe_len = printer.print_trivia_squished(pipe_trailing);
        post_pipe_len += printer.print_trivia_squished(ty_leading);
        if post_pipe_len == 0 {
            printer.print_spaces(1); // only add space if there are no block comments between
        }
        if printer
            .print(ty, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        if i + 1 < arms.len() {
            pre_pipe_len = printer.try_print_trivia_single_line_squished(ty_trailing)?;
        }
    }
    if printer.output.len() > shape.width {
        None
    } else {
        Some(PrintInfo::default_single_line())
    }
}

impl Printable for Id<UnionType> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| union_type_single_line(self, &shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
}

impl Printable for Id<OptionalType> {
    fn print(self, mut shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        shape.width = shape
            .width
            .saturating_sub(usize::from(printer.range(node.q).len()));
        let info = printer.print(node.inner, shape);
        printer.print_token(node.q);
        info
    }
}

impl Printable for Id<ArrayType> {
    fn print(self, mut shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        let close = present(node.r);
        let brackets_width = printer.range(node.l).len() + printer.range(close).len();
        shape.width = shape.width.saturating_sub(usize::from(brackets_width));
        let info = printer.print(node.inner, shape);
        printer.print_token(node.l);
        printer.print_token(close);
        info
    }
}

impl Printable for Id<QualifiedPath> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        printer.print_token(node.l);
        printer.print(node.base, shape);
        printer.print_str(" ");
        printer.print_token(node.as_kw);
        printer.print_str(" ");
        printer.print(present(node.iface), shape);
        printer.print_token(present(node.r));
        printer.print_token(present(node.dot));
        printer.print(present(node.member), shape);
        PrintInfo::default_single_line()
    }
}

impl Printable for Id<AssocTypeBinding> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        let ty = present(node.ty);
        printer.print(node.name, shape);
        let (_, equals_trailing) = printer.trivia.get_for_range_split(printer.range(node.eq));
        printer.print_str(" = ");
        printer.print_trivia_squished(equals_trailing);
        let leading = printer.trivia.get_leading_for_element(printer.tree, ty);
        printer.print_trivia_squished(leading);
        printer.print(ty, shape)
    }
}

/// `<a, b>`: one argument per line when it does not fit on one. A trailing
/// comma in the source is dropped.
fn print_angle_list_multi_line<T: Printable>(
    l: Token<punct::Less>,
    items: &[SepItem<T, punct::Comma>],
    r: Token<punct::Greater>,
    shape: &Shape,
    printer: &mut Printer,
) -> PrintInfo {
    let tree = printer.tree;
    let inner_indent = shape.indent + printer.config.indent_width;
    let inner_shape = Shape {
        width: printer.config.line_width.saturating_sub(inner_indent),
        indent: inner_indent,
        first_line_offset: 0,
    };
    let Some((first, rest)) = items.split_first() else {
        printer.print_token(l);
        printer.print_token(r);
        return PrintInfo::default_single_line();
    };
    printer.print_token(l);
    printer.print_trivia_all_trailing_for(printer.range(l));
    printer.print_newline();
    let (first_leading, first_trailing) = printer.trivia.get_for_element(tree, first.item);
    printer.print_trivia_with_newline(first_leading.trim_blanks(), inner_shape.indent);
    printer.print_spaces(inner_shape.indent);
    printer.print(first.item, inner_shape);
    if rest.is_empty() {
        printer.print_trivia_trailing(first_trailing);
        printer.print_newline();
        let (close_leading, _) = printer.trivia.get_for_range_split(printer.range(r));
        printer.print_trivia_with_newline(close_leading.trim_blanks(), inner_shape.indent);
        printer.print_spaces(shape.indent);
        printer.print_token(r);
        return PrintInfo::default_multi_lined();
    }
    let _ = printer.try_print_trivia_single_line_squished(first_trailing); // only keep if single-line block comments
    let mut previous = first;
    for (i, item) in rest.iter().enumerate() {
        let comma = previous.sep.unwrap_or_else(|| {
            unreachable!("every argument before another is followed by its comma")
        });
        let (comma_leading, comma_trailing) =
            printer.trivia.get_for_range_split(printer.range(comma));
        let _ = printer.try_print_trivia_single_line_squished(comma_leading); // only keep if single-line block comments
        printer.print_token(comma);
        printer.print_trivia_trailing(comma_trailing);
        printer.print_newline();
        let (ty_leading, ty_trailing) = printer.trivia.get_for_element(tree, item.item);
        printer.print_trivia_with_newline(ty_leading.trim_blanks(), inner_shape.indent);
        printer.print_spaces(inner_shape.indent);
        printer.print(item.item, inner_shape);
        if i + 1 < rest.len() {
            let _ = printer.try_print_trivia_single_line_squished(ty_trailing); // only keep if single-line block comments
        } else {
            printer.print_trivia_trailing(ty_trailing);
        }
        previous = item;
    }
    printer.print_newline();
    let (close_leading, _) = printer.trivia.get_for_range_split(printer.range(r));
    printer.print_trivia_with_newline(close_leading.trim_blanks(), inner_shape.indent);
    printer.print_spaces(shape.indent);
    printer.print_token(r);
    PrintInfo::default_multi_lined()
}

fn print_angle_list_single_line<T: Printable>(
    l: Token<punct::Less>,
    items: &[SepItem<T, punct::Comma>],
    r: Token<punct::Greater>,
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    let tree = printer.tree;
    printer.print_token(l);
    let (_, open_trailing) = printer.trivia.get_for_range_split(printer.range(l));
    printer.try_print_trivia_single_line_squished(open_trailing)?;
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            let comma = items[i - 1].sep.unwrap_or_else(|| {
                unreachable!("every argument before another is followed by its comma")
            });
            let (comma_leading, comma_trailing) =
                printer.trivia.get_for_range_split(printer.range(comma));
            printer.try_print_trivia_single_line_squished(comma_leading)?;
            printer.print_token(comma);
            printer.try_print_trivia_single_line_squished(comma_trailing)?;
            printer.print_str(" ");
        }
        let (leading, trailing) = printer.trivia.get_for_element(tree, item.item);
        printer.try_print_trivia_single_line_squished(leading)?;
        if printer
            .print(item.item, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        printer.try_print_trivia_single_line_squished(trailing)?;
    }
    let (close_leading, _) = printer.trivia.get_for_range_split(printer.range(r));
    printer.try_print_trivia_single_line_squished(close_leading)?;
    printer.print_token(r);
    if printer.output.len() > shape.width {
        None
    } else {
        Some(PrintInfo::default_single_line())
    }
}

impl PrintMultiLine for Id<TypeArgs> {
    fn print_multi_line(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        print_angle_list_multi_line(
            node.l,
            tree.list(node.args),
            present(node.r),
            &shape,
            printer,
        )
    }
}

impl Printable for Id<TypeArgs> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        printer
            .try_sub_printer(|p| {
                print_angle_list_single_line(
                    node.l,
                    tree.list(node.args),
                    present(node.r),
                    &shape,
                    p,
                )
            })
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
}

impl PrintMultiLine for Id<FunctionType> {
    fn print_multi_line(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let close = present(node.r);
        let ret = present(node.ret);
        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };
        printer.print_token(node.l);
        printer.print_trivia_all_trailing_for(printer.range(node.l));
        printer.print_newline();
        for SepItem { item: param, sep } in tree.list(node.params) {
            printer.print_trivia_all_leading_with_newline_for(
                param.leftmost_token(tree),
                inner_shape.indent,
            );
            printer.print_spaces(inner_shape.indent);
            printer.print(*param, inner_shape);
            if let Some(comma) = *sep {
                printer.print_token(comma);
                printer.print_trivia_all_trailing_for(printer.range(comma));
            } else {
                printer.print_str(",");
                printer.print_trivia_all_trailing_for(param.rightmost_token(tree));
            }
            printer.print_newline();
        }
        let (close_leading, _) = printer.trivia.get_for_range_split(printer.range(close));
        printer.print_trivia_with_newline(close_leading.trim_blanks(), inner_shape.indent);
        printer.print_spaces(shape.indent);
        printer.print_token(close);
        printer.print_str(" ");
        printer.print_token(node.arrow);
        printer.print_str(" ");
        printer.print(ret, shape);
        if let Some(throws) = node.throws {
            printer.print_str(" ");
            printer.print(throws, shape);
        }
        PrintInfo::default_multi_lined()
    }
}

fn function_type_single_line(
    id: Id<FunctionType>,
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    let tree = printer.tree;
    let node = tree.get(id);
    let close = present(node.r);
    let ret = present(node.ret);
    let params = tree.list(node.params);
    printer.print_token(node.l);
    let (_, open_trailing) = printer.trivia.get_for_range_split(printer.range(node.l));
    printer.try_print_trivia_single_line_squished(open_trailing)?;
    for (i, SepItem { item: param, sep }) in params.iter().enumerate() {
        if printer.output.len() > shape.width {
            return None;
        }
        let (p_leading, p_trailing) = printer.trivia.get_for_element(tree, *param);
        printer.try_print_trivia_single_line_squished(p_leading)?;
        if printer
            .print(*param, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        printer.try_print_trivia_single_line_squished(p_trailing)?;
        if i + 1 < params.len() {
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
            let (comma_leading, comma_trailing) =
                printer.trivia.get_for_range_split(printer.range(comma));
            printer.try_print_trivia_single_line_squished(comma_leading)?;
            printer.try_print_trivia_single_line_squished(comma_trailing)?;
        }
    }
    let (close_leading, _) = printer.trivia.get_for_range_split(printer.range(close));
    printer.try_print_trivia_single_line_squished(close_leading)?;
    printer.print_token(close);
    printer.print_str(" ");
    printer.print_token(node.arrow);
    printer.print_str(" ");
    if printer
        .print(ret, Shape::unlimited_single_line())
        .multi_lined
    {
        return None;
    }
    if let Some(throws) = node.throws {
        printer.print_str(" ");
        if printer
            .print(throws, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
    }
    if printer.output.len() > shape.width {
        None
    } else {
        Some(PrintInfo::default_single_line())
    }
}

impl Printable for Id<FunctionType> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| function_type_single_line(self, &shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
}

impl Printable for Id<FunctionTypeParam> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        if let Some(name) = node.name {
            printer.print(name.name, shape);
            if let Some(question) = name.question {
                printer.print_token(question);
            }
            printer.print_token(name.colon);
            printer.print_str(" ");
        }
        printer.print(present(node.ty), shape)
    }
}

impl Printable for Id<ThrowsClause> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        let ty = present(node.ty);
        printer.print_token(node.kw);
        printer.print_separator(
            printer.range(node.kw),
            Some(ty.leftmost_token(printer.tree)),
            shape.indent + printer.config.indent_width,
            " ",
        );
        let multi_lined = printer.print(ty, shape).multi_lined;
        PrintInfo { multi_lined }
    }
}
