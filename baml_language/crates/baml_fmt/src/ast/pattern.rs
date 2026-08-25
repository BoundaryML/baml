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

use baml_db::baml_compiler_syntax::validated::nodes::{
    ArrayPattern, ArrayPatternElement, BindingPattern, ChainPattern, DestructurePattern,
    DestructureTypeArgs, FieldPattern, MatchPattern, ParenPattern, TypePattern, UnionPattern,
    UnreflectPattern, WildcardPattern,
};
use rowan::TextRange;

use crate::{
    ast::{Token, tokens as t},
    printer::{PrintInfo, PrintMultiLine, Printable, Printer, Shape},
    trivia_classifier::{EmittableTrivia, TriviaSliceExt},
};

trait DestructurePatternLayout {
    fn print_path(&self, printer: &mut Printer);
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}

trait ArrayPatternLayout {
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}

trait UnionPatternLayout {
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
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

fn print_binding_keyword_with_trailing_trivia(printer: &mut Printer, keyword: &t::BindingKeyword) {
    printer.print_raw_token(keyword);
    printer.print_str(" ");
    let (_, trailing) = printer.trivia.get_for_range_split(keyword.span());
    if print_trivia_squished_spaced(printer, trailing, false, false) > 0 {
        printer.print_str(" ");
    }
}

impl Printable for MatchPattern {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            MatchPattern::Wildcard(p) => p.print(shape, printer),
            MatchPattern::Binding(p) => p.print(shape, printer),
            MatchPattern::Destructure(p) => p.print(shape, printer),
            MatchPattern::Array(p) => p.print(shape, printer),
            MatchPattern::Type(p) => p.print(shape, printer),
            MatchPattern::Unreflect(p) => p.print(shape, printer),
            MatchPattern::Paren(p) => p.print(shape, printer),
            MatchPattern::Union(p) => p.print(shape, printer),
            MatchPattern::Chain(p) => p.print(shape, printer),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            MatchPattern::Wildcard(p) => p.leftmost_token(),
            MatchPattern::Binding(p) => p.leftmost_token(),
            MatchPattern::Destructure(p) => p.leftmost_token(),
            MatchPattern::Array(p) => p.leftmost_token(),
            MatchPattern::Type(p) => p.leftmost_token(),
            MatchPattern::Unreflect(p) => p.leftmost_token(),
            MatchPattern::Paren(p) => p.leftmost_token(),
            MatchPattern::Union(p) => p.leftmost_token(),
            MatchPattern::Chain(p) => p.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            MatchPattern::Wildcard(p) => p.rightmost_token(),
            MatchPattern::Binding(p) => p.rightmost_token(),
            MatchPattern::Destructure(p) => p.rightmost_token(),
            MatchPattern::Array(p) => p.rightmost_token(),
            MatchPattern::Type(p) => p.rightmost_token(),
            MatchPattern::Unreflect(p) => p.rightmost_token(),
            MatchPattern::Paren(p) => p.rightmost_token(),
            MatchPattern::Union(p) => p.rightmost_token(),
            MatchPattern::Chain(p) => p.rightmost_token(),
        }
    }
}

// ─── Atoms ────────────────────────────────────────────────────────────────────

impl Printable for WildcardPattern {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        if let Some(let_kw) = &self.let_keyword {
            print_binding_keyword_with_trailing_trivia(printer, let_kw);
        }
        printer.print_raw_token(&self.underscore);
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.let_keyword
            .as_ref()
            .map(super::tokens::Token::span)
            .unwrap_or_else(|| self.underscore.span())
    }
    fn rightmost_token(&self) -> TextRange {
        self.underscore.span()
    }
}

impl Printable for BindingPattern {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        print_binding_keyword_with_trailing_trivia(printer, &self.let_keyword);
        printer.print_raw_token(&self.name);
        let mut info = PrintInfo::default_single_line();
        if let Some((colon, pattern)) = &self.subpat {
            let (_, name_trailing) = printer.trivia.get_for_range_split(self.name.span());
            print_trivia_squished_spaced(printer, name_trailing, true, false);
            let (colon_leading, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
            print_trivia_squished_spaced(printer, colon_leading, true, false);
            printer.print_raw_token(colon);
            printer.print_str(" ");
            print_trivia_squished_spaced(printer, colon_trailing, false, true);
            let pattern_leading = printer.trivia.get_leading_for_element(&**pattern);
            print_trivia_squished_spaced(printer, pattern_leading, false, true);
            info.multi_lined |= printer.print(&**pattern, shape).multi_lined;
        }
        info
    }
    fn leftmost_token(&self) -> TextRange {
        self.let_keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.subpat
            .as_ref()
            .map(|(_, p)| p.rightmost_token())
            .unwrap_or_else(|| self.name.span())
    }
}

impl Printable for DestructureTypeArgs {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            DestructureTypeArgs::Generic(args) => args.print(shape, printer),
            DestructureTypeArgs::Type(args) => args.print(shape, printer),
        }
    }

    fn leftmost_token(&self) -> TextRange {
        match self {
            DestructureTypeArgs::Generic(args) => args.leftmost_token(),
            DestructureTypeArgs::Type(args) => args.leftmost_token(),
        }
    }

    fn rightmost_token(&self) -> TextRange {
        match self {
            DestructureTypeArgs::Generic(args) => args.rightmost_token(),
            DestructureTypeArgs::Type(args) => args.rightmost_token(),
        }
    }
}

impl DestructurePatternLayout for DestructurePattern {
    fn print_path(&self, printer: &mut Printer) {
        if let Some(let_kw) = &self.let_keyword {
            print_binding_keyword_with_trailing_trivia(printer, let_kw);
        }
        printer.print_raw_token(&self.first);
        for (dot, word) in &self.rest {
            printer.print_raw_token(dot);
            printer.print_raw_token(word);
        }
        if let Some(generic_args) = &self.generic_args {
            printer.print(generic_args, Shape::unlimited_single_line());
        }
    }

    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        // TODO(class-destructure-format): make this line-width decision aware of
        // the surrounding pattern chain / let statement. Right now a destructure
        // pattern can fit by itself but still produce a long full statement.
        self.print_path(printer);
        printer.print_str(" ");
        printer.print_raw_token(&self.open_brace);

        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_brace.span());
        if self.fields.is_empty() {
            try_print_trivia_single_line_spaced(printer, open_trailing, true, true)?;
            let (close_leading, _) = printer.trivia.get_for_range_split(self.close_brace.span());
            try_print_trivia_single_line_spaced(printer, close_leading, true, false)?;
            printer.print_raw_token(&self.close_brace);
            return (printer.output.len() <= shape.width).then(PrintInfo::default_single_line);
        }

        printer.print_str(" ");
        try_print_trivia_single_line_spaced(printer, open_trailing, false, true)?;
        for (i, (field, comma)) in self.fields.iter().enumerate() {
            let (field_leading, field_trailing) = printer.trivia.get_for_element(field);
            try_print_trivia_single_line_spaced(printer, field_leading, false, true)?;
            if printer
                .print(field, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
            try_print_trivia_single_line_spaced(printer, field_trailing, true, false)?;
            if i + 1 < self.fields.len() {
                if let Some(comma) = comma {
                    let (comma_leading, comma_trailing) =
                        printer.trivia.get_for_range_split(comma.span());
                    try_print_trivia_single_line_spaced(printer, comma_leading, true, false)?;
                    printer.print_raw_token(comma);
                    try_print_trivia_single_line_spaced(printer, comma_trailing, true, false)?;
                } else {
                    printer.print_str(",");
                }
                printer.print_str(" ");
            } else if let Some(comma) = comma {
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(comma.span());
                try_print_trivia_single_line_spaced(printer, comma_leading, true, false)?;
                try_print_trivia_single_line_spaced(printer, comma_trailing, true, false)?;
            }
        }
        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_brace.span());
        try_print_trivia_single_line_spaced(printer, close_leading, true, false)?;
        printer.print_str(" ");
        printer.print_raw_token(&self.close_brace);

        (printer.output.len() <= shape.width).then(PrintInfo::default_single_line)
    }
}

impl PrintMultiLine for DestructurePattern {
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        self.print_path(printer);
        printer.print_str(" ");
        printer.print_raw_token(&self.open_brace);
        printer.print_trivia_all_trailing_for(self.open_brace.span());
        printer.print_newline();

        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };
        for (field, comma) in &self.fields {
            printer.print_trivia_all_leading_with_newline_for(
                field.leftmost_token(),
                inner_shape.indent,
            );
            printer.print_spaces(inner_shape.indent);
            printer.print(field, inner_shape.clone());
            let field_trailing = printer.trivia.get_trailing_for_element(field);
            print_trivia_squished_spaced(printer, field_trailing, true, false);
            if let Some(comma) = comma {
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(comma.span());
                print_trivia_squished_spaced(printer, comma_leading, true, false);
                printer.print_raw_token(comma);
                printer.print_trivia_trailing(comma_trailing);
            } else {
                printer.print_str(",");
            }
            printer.print_newline();
        }

        printer.print_trivia_all_leading_with_newline_for(self.close_brace.span(), shape.indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_brace);
        PrintInfo::default_multi_lined()
    }
}

impl Printable for DestructurePattern {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }

    fn leftmost_token(&self) -> TextRange {
        self.let_keyword
            .as_ref()
            .map(super::tokens::Token::span)
            .unwrap_or_else(|| self.first.span())
    }

    fn rightmost_token(&self) -> TextRange {
        self.close_brace.span()
    }
}

impl Printable for FieldPattern {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.name);
        if let Some((colon, pattern)) = &self.pattern {
            let (_, name_trailing) = printer.trivia.get_for_range_split(self.name.span());
            print_trivia_squished_spaced(printer, name_trailing, true, false);
            let (colon_leading, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
            print_trivia_squished_spaced(printer, colon_leading, true, false);
            printer.print_raw_token(colon);
            printer.print_str(" ");
            print_trivia_squished_spaced(printer, colon_trailing, false, true);
            let pattern_leading = printer.trivia.get_leading_for_element(pattern);
            print_trivia_squished_spaced(printer, pattern_leading, false, true);
            printer.print(pattern, shape)
        } else {
            PrintInfo::default_single_line()
        }
    }

    fn leftmost_token(&self) -> TextRange {
        self.name.span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.pattern
            .as_ref()
            .map(|(_, p)| p.rightmost_token())
            .unwrap_or_else(|| self.name.span())
    }
}

impl ArrayPatternLayout for ArrayPattern {
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.open_bracket);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_bracket.span());
        try_print_trivia_single_line_spaced(printer, open_trailing, false, true)?;

        for (idx, (element, comma)) in self.elements.iter().enumerate() {
            let (element_leading, element_trailing) = printer.trivia.get_for_element(element);
            try_print_trivia_single_line_spaced(printer, element_leading, false, true)?;
            if printer
                .print(element, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
            try_print_trivia_single_line_spaced(printer, element_trailing, true, false)?;

            if idx + 1 < self.elements.len() {
                if let Some(comma) = comma {
                    let (comma_leading, comma_trailing) =
                        printer.trivia.get_for_range_split(comma.span());
                    try_print_trivia_single_line_spaced(printer, comma_leading, true, false)?;
                    printer.print_raw_token(comma);
                    try_print_trivia_single_line_spaced(printer, comma_trailing, true, false)?;
                } else {
                    printer.print_str(",");
                }
                printer.print_str(" ");
            } else if let Some(comma) = comma {
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(comma.span());
                try_print_trivia_single_line_spaced(printer, comma_leading, true, false)?;
                try_print_trivia_single_line_spaced(printer, comma_trailing, true, false)?;
            }
        }

        let (close_leading, _) = printer
            .trivia
            .get_for_range_split(self.close_bracket.span());
        try_print_trivia_single_line_spaced(printer, close_leading, true, false)?;
        printer.print_raw_token(&self.close_bracket);

        if let Some((colon, ty)) = &self.ascription {
            let (_, close_trailing) = printer
                .trivia
                .get_for_range_split(self.close_bracket.span());
            try_print_trivia_single_line_spaced(printer, close_trailing, true, false)?;
            let (colon_leading, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
            try_print_trivia_single_line_spaced(printer, colon_leading, true, false)?;
            printer.print_raw_token(colon);
            printer.print_str(" ");
            try_print_trivia_single_line_spaced(printer, colon_trailing, false, true)?;
            if printer
                .print(ty, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
        }

        (printer.output.len() <= shape.width).then(PrintInfo::default_single_line)
    }
}

impl PrintMultiLine for ArrayPattern {
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.open_bracket);
        printer.print_trivia_all_trailing_for(self.open_bracket.span());
        printer.print_newline();

        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };

        for (element, comma) in &self.elements {
            printer.print_trivia_all_leading_with_newline_for(
                element.leftmost_token(),
                inner_shape.indent,
            );
            printer.print_spaces(inner_shape.indent);
            printer.print(element, inner_shape.clone());
            let element_trailing = printer.trivia.get_trailing_for_element(element);
            print_trivia_squished_spaced(printer, element_trailing, true, false);
            if let Some(comma) = comma {
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(comma.span());
                print_trivia_squished_spaced(printer, comma_leading, true, false);
                printer.print_raw_token(comma);
                printer.print_trivia_trailing(comma_trailing);
            } else {
                printer.print_str(",");
            }
            printer.print_newline();
        }

        printer.print_trivia_all_leading_with_newline_for(self.close_bracket.span(), shape.indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_bracket);
        if let Some((colon, ty)) = &self.ascription {
            let (_, close_trailing) = printer
                .trivia
                .get_for_range_split(self.close_bracket.span());
            print_trivia_squished_spaced(printer, close_trailing, true, false);
            let (colon_leading, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
            print_trivia_squished_spaced(printer, colon_leading, true, false);
            printer.print_raw_token(colon);
            printer.print_str(" ");
            print_trivia_squished_spaced(printer, colon_trailing, false, true);
            printer.print(ty, shape);
        }
        PrintInfo::default_multi_lined()
    }
}

impl Printable for ArrayPattern {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }

    fn leftmost_token(&self) -> TextRange {
        self.open_bracket.span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.ascription
            .as_ref()
            .map(|(_, ty)| ty.rightmost_token())
            .unwrap_or_else(|| self.close_bracket.span())
    }
}

impl Printable for ArrayPatternElement {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        if let Some(rest) = &self.rest {
            printer.print_raw_token(rest);
        }
        if let Some(pattern) = &self.pattern {
            if let Some(rest) = &self.rest {
                let (_, rest_trailing) = printer.trivia.get_for_range_split(rest.span());
                print_trivia_squished_spaced(printer, rest_trailing, true, true);
                let pattern_leading = printer.trivia.get_leading_for_element(pattern);
                print_trivia_squished_spaced(printer, pattern_leading, false, true);
            }
            printer.print(pattern, shape)
        } else {
            PrintInfo::default_single_line()
        }
    }

    fn leftmost_token(&self) -> TextRange {
        self.rest
            .as_ref()
            .map(super::tokens::Token::span)
            .or_else(|| self.pattern.as_ref().map(Printable::leftmost_token))
            .unwrap_or(TextRange::empty(0.into()))
    }

    fn rightmost_token(&self) -> TextRange {
        self.pattern
            .as_ref()
            .map(Printable::rightmost_token)
            .or_else(|| self.rest.as_ref().map(super::tokens::Token::span))
            .unwrap_or(TextRange::empty(0.into()))
    }
}

impl Printable for TypePattern {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print(&self.ty, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.ty.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.ty.rightmost_token()
    }
}

impl Printable for UnreflectPattern {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.marker);
        printer.print_raw_token(&self.open_paren);
        let info = printer.print(&*self.operand, shape);
        printer.print_raw_token(&self.close_paren);
        info
    }

    fn leftmost_token(&self) -> TextRange {
        self.marker.span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.close_paren.span()
    }
}

impl Printable for ParenPattern {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        // Preserve trivia between the parens and the inner pattern. Without
        // this, comments like `( /* hint */ Foo )` or `( Foo /* trail */ )`
        // are silently dropped — data loss and an idempotence break for
        // re-formatting. Mirrors the trivia handling in `ChainPattern`.
        printer.print_raw_token(&self.open_paren);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_paren.span());
        printer.print_trivia_squished(open_trailing);
        let pat_leading = printer.trivia.get_leading_for_element(&*self.pattern);
        printer.print_trivia_squished(pat_leading);
        let info = printer.print(&*self.pattern, shape);
        let pat_trailing = printer.trivia.get_trailing_for_element(&*self.pattern);
        printer.print_trivia_squished(pat_trailing);
        printer.print_raw_token(&self.close_paren);
        info
    }
    fn leftmost_token(&self) -> TextRange {
        self.open_paren.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_paren.span()
    }
}

// ─── Combinators ──────────────────────────────────────────────────────────────

impl UnionPatternLayout for UnionPattern {
    /// Print as `A | B | C` on a single line, preserving trivia adjacent to
    /// each `|` and to every member boundary. Mirrors `UnionType`'s
    /// trivia-aware single-line printer: block comments like `A /* hint */
    /// | B`, `A | /* hint */ B`, and `A /* end */` after the last member
    /// all round-trip cleanly. Bails to multi-line whenever any trivia
    /// would itself span lines, which keeps formatting idempotent.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        if printer
            .print(&*self.first, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        let first_trailing = printer.trivia.get_trailing_for_element(&*self.first);
        let mut pre_pipe_len = printer.try_print_trivia_single_line_squished(first_trailing)?;

        for (i, (pipe, pat)) in self.rest.iter().enumerate() {
            if printer.output.len() > shape.width {
                return None;
            }
            let (pipe_leading, pipe_trailing) = printer.trivia.get_for_range_split(pipe.span());
            let (pat_leading, pat_trailing) = printer.trivia.get_for_element(pat);
            pre_pipe_len += printer.print_trivia_squished(pipe_leading);
            if pre_pipe_len == 0 {
                printer.print_spaces(1); // no block comments between previous member and `|`
            }

            printer.print_raw_token(pipe);

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
            if i + 1 < self.rest.len() {
                pre_pipe_len = printer.try_print_trivia_single_line_squished(pat_trailing)?;
            }
        }
        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl PrintMultiLine for UnionPattern {
    /// Multi-line layout: first member on the current line, each subsequent
    /// member starts with `|` on its own indented line.
    ///
    /// ```baml
    /// FirstPattern
    ///     | SecondPattern
    ///     | ThirdPattern
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut info = printer.print(&*self.first, shape.clone());
        // Emit any line/block comments hanging off the first member's
        // closing token before we break to the next line — keeps trailing
        // comments attached to the right alternative.
        printer.print_trivia_all_trailing_for(self.first.rightmost_token());
        let inner_indent = shape.indent + printer.config.indent_width;

        for (i, (pipe, pat)) in self.rest.iter().enumerate() {
            info.multi_lined = true;
            let (pipe_leading, pipe_trailing) = printer.trivia.get_for_range_split(pipe.span());
            let (pat_leading, pat_trailing) = printer.trivia.get_for_element(pat);

            printer.print_newline();
            // Pre-pipe leading comments (e.g. `/* note */` directly before
            // `|` on its own line) get re-emitted with a hard newline so
            // they don't collide with the indented `|`.
            printer.print_trivia_with_newline(pipe_leading.trim_blanks(), inner_indent);

            printer.print_spaces(inner_indent);
            printer.print_raw_token(pipe);

            let mut post_pipe_len = printer.print_trivia_squished(pipe_trailing);
            post_pipe_len += printer.print_trivia_squished(pat_leading);
            if post_pipe_len == 0 {
                printer.print_spaces(1); // only add space if there are no block comments between
            }
            printer.print(pat, shape.clone());
            if i + 1 < self.rest.len() {
                printer.print_trivia_trailing(pat_trailing);
            }
        }
        info
    }
}

impl Printable for UnionPattern {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.first.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.rest
            .last()
            .map(|(_, p)| p.rightmost_token())
            .unwrap_or_else(|| self.first.rightmost_token())
    }
}

impl Printable for ChainPattern {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        // Accumulate multi-line state from every link so callers like
        // `LetStmt` / `ForIteratorArgs` don't mistakenly treat the chain as
        // single-line after a child emitted newlines.
        let mut info = printer.print(&*self.first, shape.clone());
        for (i, (colon, pat)) in self.rest.iter().enumerate() {
            let left_trailing = if i == 0 {
                printer.trivia.get_trailing_for_element(&*self.first)
            } else {
                printer.trivia.get_trailing_for_element(&self.rest[i - 1].1)
            };
            print_trivia_squished_spaced(printer, left_trailing, true, false);
            let (colon_leading, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
            print_trivia_squished_spaced(printer, colon_leading, true, false);
            printer.print_raw_token(colon);
            printer.print_str(" ");
            // Preserve trivia between `:` and the next pattern (block
            // comments, etc.) — mirrors the trivia handling on let-stmt
            // type annotations.
            print_trivia_squished_spaced(printer, colon_trailing, false, true);
            let pat_leading = printer.trivia.get_leading_for_element(pat);
            print_trivia_squished_spaced(printer, pat_leading, false, true);
            info.multi_lined |= printer.print(pat, shape.clone()).multi_lined;
            // Trailing trivia before the next `:` is emitted at the start of
            // the next iteration. The last link's trailing trivia belongs to
            // the surrounding context.
        }
        info
    }
    fn leftmost_token(&self) -> TextRange {
        self.first.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.rest
            .last()
            .map(|(_, p)| p.rightmost_token())
            .unwrap_or_else(|| self.first.rightmost_token())
    }
}
