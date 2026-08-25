//! Formatter AST for unified patterns.
//!
//! Mirrors the parser's pattern grammar:
//!
//! ```text
//!   PATTERN     := CHAIN
//!   CHAIN       := UNION (':' UNION)*
//!   UNION       := ATOM ('|' ATOM)*
//!   ATOM        := BINDING_PATTERN
//!                | DESTRUCTURE_PATTERN
//!                | ARRAY_PATTERN
//!                | TYPE_PATTERN
//!                | UNREFLECT_PATTERN
//!                | PAREN_PATTERN
//!                | WILDCARD_PATTERN
//! ```
//!
//! `:` (chain narrow) is split before `|` (union alternation), so
//! `let x: int | string` parses as `let x : (int | string)`.

use baml_db::baml_compiler_syntax::{
    SyntaxKind, ast as raw_ast,
    validated::{
        Validated, ValidatedChainPatternItem, ValidatedPatternAtom, ValidatedPatternKind,
        ValidatedSyntaxToken,
    },
};
use rowan::{TextRange, ast::AstNode};

use crate::{
    ast::Token,
    printer::{PrintInfo, Printable, Printer, Shape},
    trivia_classifier::EmittableTrivia,
};

impl Printable for Validated<'_, raw_ast::Pattern> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        self.pattern_kind().print(shape, printer)
    }

    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for Validated<'_, raw_ast::PatternKind> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self.as_variant() {
            ValidatedPatternKind::ChainPattern(pattern) => pattern.print(shape, printer),
            ValidatedPatternKind::UnionPattern(pattern) => pattern.print(shape, printer),
            ValidatedPatternKind::BindingPattern(pattern) => pattern.print(shape, printer),
            ValidatedPatternKind::DestructurePattern(pattern) => pattern.print(shape, printer),
            ValidatedPatternKind::ArrayPattern(pattern) => pattern.print(shape, printer),
            ValidatedPatternKind::TypePattern(pattern) => pattern.print(shape, printer),
            ValidatedPatternKind::UnreflectPattern(pattern) => pattern.print(shape, printer),
            ValidatedPatternKind::ParenPattern(pattern) => pattern.print(shape, printer),
            ValidatedPatternKind::WildcardPattern(pattern) => pattern.print(shape, printer),
        }
    }

    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for Validated<'_, raw_ast::ChainPatternItem> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self.as_variant() {
            ValidatedChainPatternItem::UnionPattern(pattern) => pattern.print(shape, printer),
            ValidatedChainPatternItem::BindingPattern(pattern) => pattern.print(shape, printer),
            ValidatedChainPatternItem::DestructurePattern(pattern) => pattern.print(shape, printer),
            ValidatedChainPatternItem::ArrayPattern(pattern) => pattern.print(shape, printer),
            ValidatedChainPatternItem::TypePattern(pattern) => pattern.print(shape, printer),
            ValidatedChainPatternItem::UnreflectPattern(pattern) => pattern.print(shape, printer),
            ValidatedChainPatternItem::ParenPattern(pattern) => pattern.print(shape, printer),
            ValidatedChainPatternItem::WildcardPattern(pattern) => pattern.print(shape, printer),
        }
    }

    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for Validated<'_, raw_ast::PatternAtom> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self.as_variant() {
            ValidatedPatternAtom::BindingPattern(pattern) => pattern.print(shape, printer),
            ValidatedPatternAtom::DestructurePattern(pattern) => pattern.print(shape, printer),
            ValidatedPatternAtom::ArrayPattern(pattern) => pattern.print(shape, printer),
            ValidatedPatternAtom::TypePattern(pattern) => pattern.print(shape, printer),
            ValidatedPatternAtom::UnreflectPattern(pattern) => pattern.print(shape, printer),
            ValidatedPatternAtom::ParenPattern(pattern) => pattern.print(shape, printer),
            ValidatedPatternAtom::WildcardPattern(pattern) => pattern.print(shape, printer),
        }
    }

    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

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

fn print_trivia_squished_spaced(
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

fn print_validated_binding_keyword(printer: &mut Printer, keyword: Option<ValidatedSyntaxToken>) {
    if let Some(keyword) = keyword {
        printer.print_raw_token(&keyword);
        printer.print_str(" ");
        let (_, trailing) = printer.trivia.get_for_range_split(keyword.span());
        if print_trivia_squished_spaced(printer, trailing, false, false) > 0 {
            printer.print_str(" ");
        }
    }
}

impl Printable for Validated<'_, raw_ast::WildcardPattern> {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        print_validated_binding_keyword(printer, self.let_token().or(self.const_token()));
        printer.print_raw_token(&self.name_token());
        PrintInfo::default_single_line()
    }

    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.name_token().span()
    }
}

impl Printable for Validated<'_, raw_ast::BindingPattern> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        print_validated_binding_keyword(printer, self.let_token().or(self.const_token()));
        let name = self.name_token();
        printer.print_raw_token(&name);
        let mut info = PrintInfo::default_single_line();
        if let Some((colon, pattern)) = self.colon_token().zip(self.pattern()) {
            print_trivia_squished_spaced(
                printer,
                printer.trivia.get_for_range_split(name.span()).1,
                true,
                false,
            );
            let (colon_leading, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
            print_trivia_squished_spaced(printer, colon_leading, true, false);
            printer.print_raw_token(&colon);
            printer.print_str(" ");
            print_trivia_squished_spaced(printer, colon_trailing, false, true);
            print_trivia_squished_spaced(
                printer,
                printer.trivia.get_leading_for_element(&pattern),
                false,
                true,
            );
            info.multi_lined |= pattern.print(shape, printer).multi_lined;
        }
        info
    }

    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.pattern().map_or_else(
            || self.name_token().span(),
            |pattern| pattern.rightmost_token(),
        )
    }
}

impl Printable for Validated<'_, raw_ast::TypePattern> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        self.type_expr().print(shape, printer)
    }

    fn leftmost_token(&self) -> TextRange {
        self.type_expr().leftmost_token()
    }

    fn rightmost_token(&self) -> TextRange {
        self.type_expr().rightmost_token()
    }
}

impl Printable for Validated<'_, raw_ast::UnreflectPattern> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.name_token());
        printer.print_raw_token(&self.l_paren_token());
        let info = self.value().print(shape, printer);
        printer.print_raw_token(&self.r_paren_token());
        info
    }

    fn leftmost_token(&self) -> TextRange {
        self.name_token().span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.r_paren_token().span()
    }
}

impl Printable for Validated<'_, raw_ast::ParenPattern> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let open = self.l_paren_token();
        let pattern = self.pattern();
        let close = self.r_paren_token();
        printer.print_raw_token(&open);
        printer.print_trivia_squished(printer.trivia.get_for_range_split(open.span()).1);
        printer.print_trivia_squished(printer.trivia.get_leading_for_element(&pattern));
        let info = pattern.print(shape, printer);
        printer.print_trivia_squished(printer.trivia.get_trailing_for_element(&pattern));
        printer.print_raw_token(&close);
        info
    }

    fn leftmost_token(&self) -> TextRange {
        self.l_paren_token().span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.r_paren_token().span()
    }
}

impl Printable for Validated<'_, raw_ast::FieldPattern> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let name = self.name_token();
        printer.print_raw_token(&name);
        let Some((colon, pattern)) = self.colon_token().zip(self.pattern()) else {
            return PrintInfo::default_single_line();
        };
        print_trivia_squished_spaced(
            printer,
            printer.trivia.get_for_range_split(name.span()).1,
            true,
            false,
        );
        let (colon_leading, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
        print_trivia_squished_spaced(printer, colon_leading, true, false);
        printer.print_raw_token(&colon);
        printer.print_str(" ");
        print_trivia_squished_spaced(printer, colon_trailing, false, true);
        print_trivia_squished_spaced(
            printer,
            printer.trivia.get_leading_for_element(&pattern),
            false,
            true,
        );
        pattern.print(shape, printer)
    }

    fn leftmost_token(&self) -> TextRange {
        self.name_token().span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.pattern().map_or_else(
            || self.name_token().span(),
            |pattern| pattern.rightmost_token(),
        )
    }
}

impl Printable for Validated<'_, raw_ast::ArrayPatternElement> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        if let Some(rest) = self.dot_dot_token() {
            printer.print_raw_token(&rest);
            if let Some(pattern) = self.pattern() {
                print_trivia_squished_spaced(
                    printer,
                    printer.trivia.get_for_range_split(rest.span()).1,
                    true,
                    true,
                );
                print_trivia_squished_spaced(
                    printer,
                    printer.trivia.get_leading_for_element(&pattern),
                    false,
                    true,
                );
                return pattern.print(shape, printer);
            }
            PrintInfo::default_single_line()
        } else if let Some(pattern) = self.pattern() {
            pattern.print(shape, printer)
        } else {
            PrintInfo::default_single_line()
        }
    }

    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

fn validated_array_pattern_items(
    pattern: Validated<'_, raw_ast::ArrayPattern>,
) -> Vec<(
    Validated<'_, raw_ast::ArrayPatternElement>,
    Option<ValidatedSyntaxToken>,
)> {
    let mut items = Vec::new();
    for element in pattern.direct_elements() {
        if let Some(item) = element.node::<raw_ast::ArrayPatternElement>() {
            items.push((item, None));
        } else if let Some(token) = element.token()
            && token.kind() == SyntaxKind::COMMA
            && let Some((_, comma)) = items.last_mut()
        {
            *comma = Some(token);
        }
    }
    items
}

fn print_validated_array_pattern_single_line(
    pattern: Validated<'_, raw_ast::ArrayPattern>,
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    let open = pattern.l_bracket_token();
    let close = pattern.r_bracket_token();
    printer.print_raw_token(&open);
    try_print_trivia_single_line_spaced(
        printer,
        printer.trivia.get_for_range_split(open.span()).1,
        false,
        true,
    )?;
    let items = validated_array_pattern_items(pattern);
    for (index, (item, comma)) in items.iter().enumerate() {
        let (leading, trailing) = printer.trivia.get_for_element(item);
        try_print_trivia_single_line_spaced(printer, leading, false, true)?;
        if item
            .print(Shape::unlimited_single_line(), printer)
            .multi_lined
        {
            return None;
        }
        try_print_trivia_single_line_spaced(printer, trailing, true, false)?;
        if index + 1 < items.len() {
            if let Some(comma) = comma {
                printer.print_raw_token(comma);
            } else {
                printer.print_str(",");
            }
            printer.print_str(" ");
        }
    }
    try_print_trivia_single_line_spaced(
        printer,
        printer.trivia.get_for_range_split(close.span()).0,
        true,
        false,
    )?;
    printer.print_raw_token(&close);
    if let Some((colon, ty)) = pattern.colon_token().zip(pattern.type_expr()) {
        printer.print_raw_token(&colon);
        printer.print_str(" ");
        ty.print(Shape::unlimited_single_line(), printer);
    }
    (printer.output.len() <= shape.width).then(PrintInfo::default_single_line)
}

impl Printable for Validated<'_, raw_ast::ArrayPattern> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|probe| {
                print_validated_array_pattern_single_line(*self, &shape, probe)
            })
            .unwrap_or_else(|| {
                let open = self.l_bracket_token();
                let close = self.r_bracket_token();
                let inner_indent = shape.indent + printer.config.indent_width;
                printer.print_raw_token(&open);
                printer.print_trivia_all_trailing_for(open.span());
                printer.print_newline();
                for (item, comma) in validated_array_pattern_items(*self) {
                    printer.print_trivia_all_leading_with_newline_for(
                        item.leftmost_token(),
                        inner_indent,
                    );
                    printer.print_spaces(inner_indent);
                    item.print(
                        Shape::standalone(printer.config.line_width, inner_indent),
                        printer,
                    );
                    if let Some(comma) = comma {
                        printer.print_raw_token(&comma);
                    } else {
                        printer.print_str(",");
                    }
                    printer.print_newline();
                }
                printer.print_trivia_all_leading_with_newline_for(close.span(), shape.indent);
                printer.print_spaces(shape.indent);
                printer.print_raw_token(&close);
                if let Some((colon, ty)) = self.colon_token().zip(self.type_expr()) {
                    printer.print_raw_token(&colon);
                    printer.print_str(" ");
                    ty.print(shape, printer);
                }
                PrintInfo::default_multi_lined()
            })
    }

    fn leftmost_token(&self) -> TextRange {
        self.l_bracket_token().span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.type_expr()
            .map_or_else(|| self.r_bracket_token().span(), |ty| ty.rightmost_token())
    }
}

impl Printable for Validated<'_, raw_ast::ChainPattern> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let first = self.first();
        let mut info = first.print(shape.clone(), printer);
        let mut previous = first.rightmost_token();
        for (colon, pattern) in self.colon_tokens().zip(self.rest()) {
            print_trivia_squished_spaced(
                printer,
                printer.trivia.get_for_range_split(previous).1,
                true,
                false,
            );
            let (colon_leading, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
            print_trivia_squished_spaced(printer, colon_leading, true, false);
            printer.print_raw_token(&colon);
            printer.print_str(" ");
            print_trivia_squished_spaced(printer, colon_trailing, false, true);
            print_trivia_squished_spaced(
                printer,
                printer.trivia.get_leading_for_element(&pattern),
                false,
                true,
            );
            info.multi_lined |= pattern.print(shape.clone(), printer).multi_lined;
            previous = pattern.rightmost_token();
        }
        info
    }

    fn leftmost_token(&self) -> TextRange {
        self.first().leftmost_token()
    }

    fn rightmost_token(&self) -> TextRange {
        self.rest().last().map_or_else(
            || self.first().rightmost_token(),
            |item| item.rightmost_token(),
        )
    }
}

fn print_validated_union_single_line(
    pattern: Validated<'_, raw_ast::UnionPattern>,
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    let first = pattern.first();
    if first
        .print(Shape::unlimited_single_line(), printer)
        .multi_lined
    {
        return None;
    }
    let mut previous = first.rightmost_token();
    for (pipe, item) in pattern.pipe_tokens().zip(pattern.rest()) {
        if printer.output.len() > shape.width {
            return None;
        }
        let (pipe_leading, pipe_trailing) = printer.trivia.get_for_range_split(pipe.span());
        let mut before = printer.try_print_trivia_single_line_squished(
            printer.trivia.get_for_range_split(previous).1,
        )?;
        before += printer.print_trivia_squished(pipe_leading);
        if before == 0 {
            printer.print_str(" ");
        }
        printer.print_raw_token(&pipe);
        let mut after = printer.print_trivia_squished(pipe_trailing);
        after += printer.print_trivia_squished(printer.trivia.get_leading_for_element(&item));
        if after == 0 {
            printer.print_str(" ");
        }
        if item
            .print(Shape::unlimited_single_line(), printer)
            .multi_lined
        {
            return None;
        }
        previous = item.rightmost_token();
    }
    (printer.output.len() <= shape.width).then(PrintInfo::default_single_line)
}

impl Printable for Validated<'_, raw_ast::UnionPattern> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|probe| print_validated_union_single_line(*self, &shape, probe))
            .unwrap_or_else(|| {
                let first = self.first();
                let mut info = first.print(shape.clone(), printer);
                let inner_indent = shape.indent + printer.config.indent_width;
                let mut previous = first.rightmost_token();
                for (pipe, item) in self.pipe_tokens().zip(self.rest()) {
                    printer.print_trivia_all_trailing_for(previous);
                    printer.print_newline();
                    printer.print_spaces(inner_indent);
                    printer.print_raw_token(&pipe);
                    printer.print_str(" ");
                    item.print(shape.clone(), printer);
                    previous = item.rightmost_token();
                    info.multi_lined = true;
                }
                info
            })
    }

    fn leftmost_token(&self) -> TextRange {
        self.first().leftmost_token()
    }

    fn rightmost_token(&self) -> TextRange {
        self.rest().last().map_or_else(
            || self.first().rightmost_token(),
            |item| item.rightmost_token(),
        )
    }
}

fn validated_destructure_fields(
    pattern: Validated<'_, raw_ast::DestructurePattern>,
) -> Vec<(
    Validated<'_, raw_ast::FieldPattern>,
    Option<ValidatedSyntaxToken>,
)> {
    let mut fields = Vec::new();
    for element in pattern.direct_elements() {
        if let Some(field) = element.node::<raw_ast::FieldPattern>() {
            fields.push((field, None));
        } else if let Some(token) = element.token()
            && token.kind() == SyntaxKind::COMMA
            && let Some((_, comma)) = fields.last_mut()
        {
            *comma = Some(token);
        }
    }
    fields
}

fn print_validated_destructure_path(
    pattern: Validated<'_, raw_ast::DestructurePattern>,
    printer: &mut Printer,
) {
    print_validated_binding_keyword(printer, pattern.let_token().or(pattern.const_token()));
    printer.print_raw_token(&pattern.first_token());
    for (dot, name) in pattern.dot_tokens().zip(pattern.path_tokens()) {
        printer.print_raw_token(&dot);
        printer.print_raw_token(&name);
    }
    if let Some(args) = pattern.generic_args() {
        args.print(Shape::unlimited_single_line(), printer);
    } else if let Some(args) = pattern.type_args() {
        raw_ast::TypeArgs::cast(args.syntax().clone())
            .expect("validated destructure type arguments")
            .print(Shape::unlimited_single_line(), printer);
    }
}

fn print_validated_destructure_single_line(
    pattern: Validated<'_, raw_ast::DestructurePattern>,
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    print_validated_destructure_path(pattern, printer);
    printer.print_str(" ");
    let open = pattern.l_brace_token();
    let close = pattern.r_brace_token();
    let fields = validated_destructure_fields(pattern);
    printer.print_raw_token(&open);
    let open_trailing = printer.trivia.get_for_range_split(open.span()).1;
    if fields.is_empty() {
        try_print_trivia_single_line_spaced(printer, open_trailing, true, true)?;
        try_print_trivia_single_line_spaced(
            printer,
            printer.trivia.get_for_range_split(close.span()).0,
            true,
            false,
        )?;
        printer.print_raw_token(&close);
        return (printer.output.len() <= shape.width).then(PrintInfo::default_single_line);
    }
    printer.print_str(" ");
    try_print_trivia_single_line_spaced(printer, open_trailing, false, true)?;
    for (index, (field, comma)) in fields.iter().enumerate() {
        let (leading, trailing) = printer.trivia.get_for_element(field);
        try_print_trivia_single_line_spaced(printer, leading, false, true)?;
        if field
            .print(Shape::unlimited_single_line(), printer)
            .multi_lined
        {
            return None;
        }
        try_print_trivia_single_line_spaced(printer, trailing, true, false)?;
        if index + 1 < fields.len() {
            if let Some(comma) = comma {
                printer.print_raw_token(comma);
            } else {
                printer.print_str(",");
            }
            printer.print_str(" ");
        }
    }
    try_print_trivia_single_line_spaced(
        printer,
        printer.trivia.get_for_range_split(close.span()).0,
        true,
        false,
    )?;
    printer.print_str(" ");
    printer.print_raw_token(&close);
    (printer.output.len() <= shape.width).then(PrintInfo::default_single_line)
}

impl Printable for Validated<'_, raw_ast::DestructurePattern> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|probe| print_validated_destructure_single_line(*self, &shape, probe))
            .unwrap_or_else(|| {
                print_validated_destructure_path(*self, printer);
                printer.print_str(" ");
                let open = self.l_brace_token();
                let close = self.r_brace_token();
                let inner_indent = shape.indent + printer.config.indent_width;
                printer.print_raw_token(&open);
                printer.print_trivia_all_trailing_for(open.span());
                printer.print_newline();
                for (field, comma) in validated_destructure_fields(*self) {
                    printer.print_trivia_all_leading_with_newline_for(
                        field.leftmost_token(),
                        inner_indent,
                    );
                    printer.print_spaces(inner_indent);
                    field.print(
                        Shape::standalone(printer.config.line_width, inner_indent),
                        printer,
                    );
                    if let Some(comma) = comma {
                        printer.print_raw_token(&comma);
                    } else {
                        printer.print_str(",");
                    }
                    printer.print_newline();
                }
                printer.print_trivia_all_leading_with_newline_for(close.span(), shape.indent);
                printer.print_spaces(shape.indent);
                printer.print_raw_token(&close);
                PrintInfo::default_multi_lined()
            })
    }

    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.r_brace_token().span()
    }
}
