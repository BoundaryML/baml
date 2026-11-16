use baml_syntax::{
    nodes::{
        AnyArrayPatternElement, AnyPattern, ArrayPattern, BindingPattern, DestructurePattern,
        FieldPattern, ParenPattern, PatternAscription, RestPattern, TypeAscription, TypePattern,
        UnionPattern, WildcardPattern,
    },
    token::{Token, sets::LetIntroducer},
    tree::{Id, SepItem},
};
use text_size::TextRange;

use super::{present, union_printable};
use crate::{
    printer::{PrintInfo, PrintMultiLine, Printable, Printer, Shape, Spanned},
    trivia_classifier::{EmittableTrivia, TriviaSliceExt as _},
};

fn try_print_trivia_single_line_spaced(
    printer: &mut Printer,
    trivia: &[EmittableTrivia],
    before: bool,
    after: bool,
) -> Option<usize> {
    let trivia_len = trivia
        .iter()
        .map(|t| t.single_line_len(printer.input))
        .sum::<Option<usize>>()?;
    let comments = trivia
        .iter()
        .filter(|t| t.is_comment() && t.single_line_len(printer.input).is_some())
        .collect::<Vec<_>>();
    if !comments.is_empty() && before {
        printer.print_str(" ");
    }
    for (i, t) in comments.iter().enumerate() {
        if i > 0 {
            printer.print_str(" ");
        }
        printer.print_trivia(t);
    }
    if !comments.is_empty() && after {
        printer.print_str(" ");
    }
    Some(trivia_len)
}

pub(crate) fn print_trivia_squished_spaced(
    printer: &mut Printer,
    trivia: &[EmittableTrivia],
    before: bool,
    after: bool,
) -> usize {
    let trivia_len = trivia
        .iter()
        .filter_map(|t| t.single_line_len(printer.input))
        .sum::<usize>();
    let comments = trivia
        .iter()
        .filter(|t| t.is_comment() && t.single_line_len(printer.input).is_some())
        .collect::<Vec<_>>();
    if !comments.is_empty() && before {
        printer.print_str(" ");
    }
    for (i, t) in comments.iter().enumerate() {
        if i > 0 {
            printer.print_str(" ");
        }
        printer.print_trivia(t);
    }
    if !comments.is_empty() && after {
        printer.print_str(" ");
    }
    trivia_len
}

/// `let`/`const` followed by one space and any same-line comments.
pub(crate) fn print_binding_keyword_with_trailing_trivia(
    printer: &mut Printer,
    keyword: Token<LetIntroducer>,
) {
    printer.print_token(keyword);
    printer.print_str(" ");
    let (_, trailing) = printer.trivia.get_for_range_split(printer.range(keyword));
    if print_trivia_squished_spaced(printer, trailing, false, false) > 0 {
        printer.print_str(" ");
    }
}

union_printable!(AnyPattern {
    Binding,
    Wildcard,
    Destructure,
    Array,
    Type,
    Paren,
    Union,
});

union_printable!(AnyArrayPatternElement { Pattern, Rest });

impl Printable for Id<WildcardPattern> {
    fn print(self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        if let Some(let_kw) = node.introducer {
            print_binding_keyword_with_trailing_trivia(printer, let_kw);
        }
        printer.print_token(node.underscore);
        PrintInfo::default_single_line()
    }
}

/// `: pattern` after a name, comments between kept on the line.
fn print_pattern_ascription(
    name_range: TextRange,
    ascription: PatternAscription,
    shape: Shape,
    printer: &mut Printer,
) -> PrintInfo {
    let pattern = present(ascription.pattern);
    let (_, name_trailing) = printer.trivia.get_for_range_split(name_range);
    print_trivia_squished_spaced(printer, name_trailing, true, false);
    let (colon_leading, colon_trailing) = printer
        .trivia
        .get_for_range_split(printer.range(ascription.colon));
    print_trivia_squished_spaced(printer, colon_leading, true, false);
    printer.print_token(ascription.colon);
    printer.print_str(" ");
    print_trivia_squished_spaced(printer, colon_trailing, false, true);
    let pattern_leading = printer
        .trivia
        .get_leading_for_element(printer.tree, pattern);
    print_trivia_squished_spaced(printer, pattern_leading, false, true);
    printer.print(pattern, shape)
}

impl Printable for Id<BindingPattern> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        if let Some(let_kw) = node.introducer {
            print_binding_keyword_with_trailing_trivia(printer, let_kw);
        }
        printer.print(node.name, shape);
        let mut info = PrintInfo::default_single_line();
        if let Some(ascription) = node.ascription {
            let name_range = node.name.rightmost_token(printer.tree);
            info.multi_lined |=
                print_pattern_ascription(name_range, ascription, shape, printer).multi_lined;
        }
        info
    }
}

fn destructure_print_path(id: Id<DestructurePattern>, printer: &mut Printer) {
    let node = printer.tree.get(id);
    if let Some(let_kw) = node.introducer {
        print_binding_keyword_with_trailing_trivia(printer, let_kw);
    }
    printer.print(node.path, Shape::unlimited_single_line());
}

fn destructure_single_line(
    id: Id<DestructurePattern>,
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    let tree = printer.tree;
    let node = tree.get(id);
    let close = present(node.r);
    let fields = tree.list(node.fields);
    destructure_print_path(id, printer);
    printer.print_str(" ");
    printer.print_token(node.l);
    let (_, open_trailing) = printer.trivia.get_for_range_split(printer.range(node.l));
    if fields.is_empty() {
        try_print_trivia_single_line_spaced(printer, open_trailing, true, true)?;
        let (close_leading, _) = printer.trivia.get_for_range_split(printer.range(close));
        try_print_trivia_single_line_spaced(printer, close_leading, true, false)?;
        printer.print_token(close);
        return (printer.output.len() <= shape.width).then(PrintInfo::default_single_line);
    }
    printer.print_str(" ");
    try_print_trivia_single_line_spaced(printer, open_trailing, false, true)?;
    for (i, SepItem { item: field, sep }) in fields.iter().enumerate() {
        let (field_leading, field_trailing) = printer.trivia.get_for_element(tree, *field);
        try_print_trivia_single_line_spaced(printer, field_leading, false, true)?;
        if printer
            .print(*field, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        try_print_trivia_single_line_spaced(printer, field_trailing, true, false)?;
        if i + 1 < fields.len() {
            if let Some(comma) = *sep {
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(printer.range(comma));
                try_print_trivia_single_line_spaced(printer, comma_leading, true, false)?;
                printer.print_token(comma);
                try_print_trivia_single_line_spaced(printer, comma_trailing, true, false)?;
            } else {
                printer.print_str(",");
            }
            printer.print_str(" ");
        } else if let Some(comma) = *sep {
            let (comma_leading, comma_trailing) =
                printer.trivia.get_for_range_split(printer.range(comma));
            try_print_trivia_single_line_spaced(printer, comma_leading, true, false)?;
            try_print_trivia_single_line_spaced(printer, comma_trailing, true, false)?;
        }
    }
    let (close_leading, _) = printer.trivia.get_for_range_split(printer.range(close));
    try_print_trivia_single_line_spaced(printer, close_leading, true, false)?;
    printer.print_str(" ");
    printer.print_token(close);
    (printer.output.len() <= shape.width).then(PrintInfo::default_single_line)
}

impl PrintMultiLine for Id<DestructurePattern> {
    fn print_multi_line(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let close = present(node.r);
        destructure_print_path(self, printer);
        printer.print_str(" ");
        printer.print_token(node.l);
        printer.print_trivia_all_trailing_for(printer.range(node.l));
        printer.print_newline();
        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };
        for SepItem { item: field, sep } in tree.list(node.fields) {
            printer.print_trivia_all_leading_with_newline_for(
                field.leftmost_token(tree),
                inner_shape.indent,
            );
            printer.print_spaces(inner_shape.indent);
            printer.print(*field, inner_shape);
            let field_trailing = printer.trivia.get_trailing_for_element(tree, *field);
            print_trivia_squished_spaced(printer, field_trailing, true, false);
            if let Some(comma) = *sep {
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(printer.range(comma));
                print_trivia_squished_spaced(printer, comma_leading, true, false);
                printer.print_token(comma);
                printer.print_trivia_trailing(comma_trailing);
            } else {
                printer.print_str(",");
            }
            printer.print_newline();
        }
        printer.print_trivia_all_leading_with_newline_for(printer.range(close), shape.indent);
        printer.print_spaces(shape.indent);
        printer.print_token(close);
        PrintInfo::default_multi_lined()
    }
}

impl Printable for Id<DestructurePattern> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| destructure_single_line(self, &shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
}

impl Printable for Id<FieldPattern> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        printer.print(node.name, shape);
        if let Some(ascription) = node.sub {
            let name_range = node.name.rightmost_token(printer.tree);
            print_pattern_ascription(name_range, ascription, shape, printer)
        } else {
            PrintInfo::default_single_line()
        }
    }
}

/// `: T` after `]`.
fn print_array_ascription(
    close: TextRange,
    ascription: Id<TypeAscription>,
    shape: Shape,
    printer: &mut Printer,
    single_line: bool,
) -> Option<PrintInfo> {
    let node = printer.tree.get(ascription);
    let ty = present(node.ty);
    let (_, close_trailing) = printer.trivia.get_for_range_split(close);
    if single_line {
        try_print_trivia_single_line_spaced(printer, close_trailing, true, false)?;
    } else {
        print_trivia_squished_spaced(printer, close_trailing, true, false);
    }
    if let Some(colon) = node.colon {
        let (colon_leading, colon_trailing) =
            printer.trivia.get_for_range_split(printer.range(colon));
        if single_line {
            try_print_trivia_single_line_spaced(printer, colon_leading, true, false)?;
        } else {
            print_trivia_squished_spaced(printer, colon_leading, true, false);
        }
        printer.print_token(colon);
        printer.print_str(" ");
        if single_line {
            try_print_trivia_single_line_spaced(printer, colon_trailing, false, true)?;
        } else {
            print_trivia_squished_spaced(printer, colon_trailing, false, true);
        }
    } else {
        printer.print_str(": ");
    }
    if single_line {
        if printer
            .print(ty, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        Some(PrintInfo::default_single_line())
    } else {
        Some(printer.print(ty, shape))
    }
}

fn array_pattern_single_line(
    id: Id<ArrayPattern>,
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    let tree = printer.tree;
    let node = tree.get(id);
    let close = present(node.r);
    let elements = tree.list(node.elements);
    printer.print_token(node.l);
    let (_, open_trailing) = printer.trivia.get_for_range_split(printer.range(node.l));
    try_print_trivia_single_line_spaced(printer, open_trailing, false, true)?;
    for (idx, SepItem { item: element, sep }) in elements.iter().enumerate() {
        let (element_leading, element_trailing) = printer.trivia.get_for_element(tree, *element);
        try_print_trivia_single_line_spaced(printer, element_leading, false, true)?;
        if printer
            .print(*element, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        try_print_trivia_single_line_spaced(printer, element_trailing, true, false)?;
        if idx + 1 < elements.len() {
            if let Some(comma) = *sep {
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(printer.range(comma));
                try_print_trivia_single_line_spaced(printer, comma_leading, true, false)?;
                printer.print_token(comma);
                try_print_trivia_single_line_spaced(printer, comma_trailing, true, false)?;
            } else {
                printer.print_str(",");
            }
            printer.print_str(" ");
        } else if let Some(comma) = *sep {
            let (comma_leading, comma_trailing) =
                printer.trivia.get_for_range_split(printer.range(comma));
            try_print_trivia_single_line_spaced(printer, comma_leading, true, false)?;
            try_print_trivia_single_line_spaced(printer, comma_trailing, true, false)?;
        }
    }
    let (close_leading, _) = printer.trivia.get_for_range_split(printer.range(close));
    try_print_trivia_single_line_spaced(printer, close_leading, true, false)?;
    printer.print_token(close);
    if let Some(ascription) = node.ascription {
        print_array_ascription(printer.range(close), ascription, *shape, printer, true)?;
    }
    (printer.output.len() <= shape.width).then(PrintInfo::default_single_line)
}

impl PrintMultiLine for Id<ArrayPattern> {
    fn print_multi_line(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let close = present(node.r);
        printer.print_token(node.l);
        printer.print_trivia_all_trailing_for(printer.range(node.l));
        printer.print_newline();
        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };
        for SepItem { item: element, sep } in tree.list(node.elements) {
            printer.print_trivia_all_leading_with_newline_for(
                element.leftmost_token(tree),
                inner_shape.indent,
            );
            printer.print_spaces(inner_shape.indent);
            printer.print(*element, inner_shape);
            let element_trailing = printer.trivia.get_trailing_for_element(tree, *element);
            print_trivia_squished_spaced(printer, element_trailing, true, false);
            if let Some(comma) = *sep {
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(printer.range(comma));
                print_trivia_squished_spaced(printer, comma_leading, true, false);
                printer.print_token(comma);
                printer.print_trivia_trailing(comma_trailing);
            } else {
                printer.print_str(",");
            }
            printer.print_newline();
        }
        printer.print_trivia_all_leading_with_newline_for(printer.range(close), shape.indent);
        printer.print_spaces(shape.indent);
        printer.print_token(close);
        if let Some(ascription) = node.ascription {
            print_array_ascription(printer.range(close), ascription, shape, printer, false);
        }
        PrintInfo::default_multi_lined()
    }
}

impl Printable for Id<ArrayPattern> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| array_pattern_single_line(self, &shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
}

impl Printable for Id<RestPattern> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        printer.print_token(node.dots);
        if let Some(pattern) = node.pattern {
            let (_, rest_trailing) = printer.trivia.get_for_range_split(printer.range(node.dots));
            print_trivia_squished_spaced(printer, rest_trailing, true, true);
            let pattern_leading = printer
                .trivia
                .get_leading_for_element(printer.tree, pattern);
            print_trivia_squished_spaced(printer, pattern_leading, false, true);
            printer.print(pattern, shape)
        } else {
            PrintInfo::default_single_line()
        }
    }
}

impl Printable for Id<TypePattern> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print(printer.tree.get(self).ty, shape)
    }
}

impl Printable for Id<ParenPattern> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        let inner = present(node.inner);
        let close = present(node.r);
        printer.print_token(node.l);
        let (_, open_trailing) = printer.trivia.get_for_range_split(printer.range(node.l));
        printer.print_trivia_squished(open_trailing);
        let pat_leading = printer.trivia.get_leading_for_element(printer.tree, inner);
        printer.print_trivia_squished(pat_leading);
        let info = printer.print(inner, shape);
        let pat_trailing = printer.trivia.get_trailing_for_element(printer.tree, inner);
        printer.print_trivia_squished(pat_trailing);
        printer.print_token(close);
        info
    }
}

fn union_pattern_single_line(
    id: Id<UnionPattern>,
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
        let pat = present(arm.pattern);
        if printer.output.len() > shape.width {
            return None;
        }
        let (pipe_leading, pipe_trailing) =
            printer.trivia.get_for_range_split(printer.range(arm.pipe));
        let (pat_leading, pat_trailing) = printer.trivia.get_for_element(tree, pat);
        pre_pipe_len += printer.print_trivia_squished(pipe_leading);
        if pre_pipe_len == 0 {
            printer.print_spaces(1); // no block comments between previous member and `|`
        }
        printer.print_token(arm.pipe);
        let mut post_pipe_len = printer.print_trivia_squished(pipe_trailing);
        post_pipe_len += printer.print_trivia_squished(pat_leading);
        if post_pipe_len == 0 {
            printer.print_spaces(1); // no block comments between `|` and next member
        }
        if printer
            .print(pat, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        if i + 1 < arms.len() {
            pre_pipe_len = printer.try_print_trivia_single_line_squished(pat_trailing)?;
        }
    }
    if printer.output.len() > shape.width {
        None
    } else {
        Some(PrintInfo::default_single_line())
    }
}

impl PrintMultiLine for Id<UnionPattern> {
    fn print_multi_line(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let arms = tree.list(node.arms.as_list());
        let mut info = printer.print(node.first, shape);
        printer.print_trivia_all_trailing_for(node.first.rightmost_token(tree));
        let inner_indent = shape.indent + printer.config.indent_width;
        for (i, arm) in arms.iter().enumerate() {
            let pat = present(arm.pattern);
            info.multi_lined = true;
            let (pipe_leading, pipe_trailing) =
                printer.trivia.get_for_range_split(printer.range(arm.pipe));
            let (pat_leading, pat_trailing) = printer.trivia.get_for_element(tree, pat);
            printer.print_newline();
            printer.print_trivia_with_newline(pipe_leading.trim_blanks(), inner_indent);
            printer.print_spaces(inner_indent);
            printer.print_token(arm.pipe);
            let mut post_pipe_len = printer.print_trivia_squished(pipe_trailing);
            post_pipe_len += printer.print_trivia_squished(pat_leading);
            if post_pipe_len == 0 {
                printer.print_spaces(1); // only add space if there are no block comments between
            }
            printer.print(pat, shape);
            if i + 1 < arms.len() {
                printer.print_trivia_trailing(pat_trailing);
            }
        }
        info
    }
}

impl Printable for Id<UnionPattern> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| union_pattern_single_line(self, &shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
}
