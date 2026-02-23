//! Reference: [`baml_db::baml_compiler_syntax::ast::MatchPattern`] and [`baml_db::baml_compiler_hir::body::Pattern`]

use baml_db::baml_compiler_syntax::{SyntaxElement, SyntaxKind};
use rowan::TextRange;

use crate::{
    ast::{FromCST, KnownKind, Literal, StrongAstError, SyntaxNodeIter, Token, Type, tokens as t},
    printer::{PrintInfo, PrintMultiLine, Printable, Printer, Shape},
};

/// Corresponds to a [`SyntaxKind::MATCH_PATTERN`] node.
///
/// Note that unlike in the HIR, `true`/`false`/`null` are handled by the binding as words, rather than literals.
/// This shouldn't matter for formatting, but you can change if you have a use case.
#[derive(Debug)]
pub enum MatchPattern {
    Literal(Literal),
    Binding(BindingPattern),
    Path(PathPattern),
    Union(UnionPattern),
    /// This would be a top-level nested pattern, meaning there are parentheses around the whole thing
    Nested(NestedPattern),
}

impl FromCST for MatchPattern {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::MATCH_PATTERN)?;

        let mut it = SyntaxNodeIter::new(&node);

        let first_elem = UnionPatternMember::take(&mut it)?;

        if let Some(colon) = it.next_if_kind(SyntaxKind::COLON)
            && let UnionPatternMember::Word(binding_name) = first_elem
        {
            let colon = t::Colon::from_cst(colon)?;
            let ty = it.expect_parse()?;

            it.expect_end()?;

            return Ok(MatchPattern::Binding(BindingPattern {
                name: binding_name,
                ty: Some((colon, ty)),
            }));
        }

        let mut rest = Vec::new();
        while let Some(pipe) = it.next() {
            let pipe = t::Pipe::from_cst(pipe)?;
            let next = UnionPatternMember::take(&mut it)?;
            rest.push((pipe, next));
        }

        let ty = if rest.is_empty() {
            match first_elem {
                UnionPatternMember::Literal(lit) => MatchPattern::Literal(lit),
                UnionPatternMember::PathVariant(variant) => MatchPattern::Path(variant),
                UnionPatternMember::Word(word) => MatchPattern::Binding(BindingPattern {
                    name: word,
                    ty: None,
                }),
                UnionPatternMember::Nested(nested) => MatchPattern::Nested(nested),
            }
        } else {
            MatchPattern::Union(UnionPattern {
                first: Box::new(first_elem),
                rest,
            })
        };

        Ok(ty)
    }
}

impl KnownKind for MatchPattern {
    fn kind() -> SyntaxKind {
        SyntaxKind::MATCH_PATTERN
    }
}

impl Printable for MatchPattern {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            MatchPattern::Literal(lit) => printer.print(lit, shape),
            MatchPattern::Binding(binding) => binding.print(shape, printer),
            MatchPattern::Path(variant) => variant.print(shape, printer),
            MatchPattern::Union(union) => union.print(shape, printer),
            MatchPattern::Nested(nested) => nested.print(shape, printer),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            MatchPattern::Literal(lit) => lit.leftmost_token(),
            MatchPattern::Binding(binding) => binding.leftmost_token(),
            MatchPattern::Path(variant) => variant.leftmost_token(),
            MatchPattern::Union(union) => union.leftmost_token(),
            MatchPattern::Nested(nested) => nested.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            MatchPattern::Literal(lit) => lit.rightmost_token(),
            MatchPattern::Binding(binding) => binding.rightmost_token(),
            MatchPattern::Path(variant) => variant.rightmost_token(),
            MatchPattern::Union(union) => union.rightmost_token(),
            MatchPattern::Nested(nested) => nested.rightmost_token(),
        }
    }
}

#[derive(Debug)]
pub struct BindingPattern {
    pub name: t::Word,
    pub ty: Option<(t::Colon, Type)>,
}

impl Printable for BindingPattern {
    fn print(&self, mut shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.name);
        if let Some((colon, ty)) = &self.ty {
            printer.print_raw_token(colon);
            let (_, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
            printer.print_str(" ");
            let mut trivia_len = printer.print_trivia_squished(colon_trailing);
            let ty_leading = printer.trivia.get_leading_for_element(ty);
            trivia_len += printer.print_trivia_squished(ty_leading);
            let new_overhead =
                usize::from(self.name.span().len() + colon.span().len()) + 1 + trivia_len;
            shape.width = shape.width.saturating_sub(new_overhead);
            shape.first_line_offset += new_overhead;
            printer.print(ty, shape)
        } else {
            PrintInfo::default_single_line()
        }
    }
    fn leftmost_token(&self) -> TextRange {
        self.name.span()
    }
    fn rightmost_token(&self) -> TextRange {
        if let Some((_, ty)) = &self.ty {
            ty.rightmost_token()
        } else {
            self.name.span()
        }
    }
}

#[derive(Debug)]
pub struct PathPattern {
    pub first: t::Word,
    pub rest: Vec<(t::Dot, t::Word)>,
}

impl Printable for PathPattern {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.first);
        for (dot, word) in &self.rest {
            printer.print_raw_token(dot);
            printer.print_raw_token(word);
        }
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.first.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.rest
            .last()
            .map_or(&self.first, |(_, word)| word)
            .span()
    }
}

#[derive(Debug)]
pub struct UnionPattern {
    pub first: Box<UnionPatternMember>,
    pub rest: Vec<(t::Pipe, UnionPatternMember)>,
}

impl UnionPattern {
    /// Returns an iterator over the members in the union pattern, in order.
    pub fn iter_patterns(&self) -> impl Iterator<Item = &UnionPatternMember> {
        std::iter::once(&*self.first).chain(self.rest.iter().map(|(_, p)| p))
    }
}

impl PrintMultiLine for UnionPattern {
    /// Multi-line layout: first member stays on the current line, each subsequent
    /// member starts with `|` on its own indented line. Same layout as [`super::UnionType`].
    /// Trailing comments on members are preserved.
    ///
    /// ```baml
    /// FirstPattern
    ///     | SecondPattern
    ///     | ThirdPattern
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut info = printer.print(&*self.first, shape.clone());
        let inner_indent = shape.indent + printer.config.indent_width;
        printer.print_trivia_all_trailing_for(self.first.rightmost_token());
        for (i, (pipe, pattern)) in self.rest.iter().enumerate() {
            info.multi_lined = true;
            printer.print_newline();
            let (pipe_leading, pipe_trailing) = printer.trivia.get_for_range_split(pipe.span());
            let (pattern_leading, pattern_trailing) = printer.trivia.get_for_element(pattern);

            printer.print_trivia_with_newline(pipe_leading, inner_indent);
            printer.print_spaces(shape.indent + printer.config.indent_width);

            printer.print_raw_token(pipe);
            let mut post_pipe_len = printer.print_trivia_squished(pipe_trailing);
            post_pipe_len += printer.print_trivia_squished(pattern_leading);
            if post_pipe_len == 0 {
                printer.print_spaces(1); // only add space if there are no block comments between
            }

            printer.print(pattern, shape.clone());
            if i + 1 < self.rest.len() {
                printer.print_trivia_trailing(pattern_trailing);
            }
        }
        info
    }
}

impl UnionPattern {
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the union pattern on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        if self
            .first
            .print(Shape::unlimited_single_line(), printer)
            .multi_lined
        {
            return None;
        }
        let first_trailing = printer.trivia.get_trailing_for_element(&*self.first);
        let mut pre_pipe_len = printer.try_print_trivia_single_line_squished(first_trailing)?;

        for (i, (pipe, pattern)) in self.rest.iter().enumerate() {
            if printer.output.len() > shape.width {
                return None; // early abort
            }
            let (pipe_leading, pipe_trailing) = printer.trivia.get_for_range_split(pipe.span());
            let (pattern_leading, pattern_trailing) = printer.trivia.get_for_element(pattern);

            pre_pipe_len += printer.try_print_trivia_single_line_squished(pipe_leading)?; // could be put on preceding lines
            if pre_pipe_len == 0 {
                printer.print_spaces(1); // only add space if there are no block comments between
            }

            printer.print_raw_token(pipe);

            let mut post_pipe_len = printer.print_trivia_squished(pipe_trailing);
            post_pipe_len += printer.print_trivia_squished(pattern_leading);
            if post_pipe_len == 0 {
                printer.print_spaces(1); // only add space if there are no block comments between
            }

            if pattern
                .print(Shape::unlimited_single_line(), printer)
                .multi_lined
            {
                return None;
            }
            if i + 1 < self.rest.len() {
                pre_pipe_len = printer.try_print_trivia_single_line_squished(pattern_trailing)?;
            }
        }

        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
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
            .map_or(&*self.first, |(_, member)| member)
            .rightmost_token()
    }
}

#[derive(Debug)]
pub enum UnionPatternMember {
    Literal(Literal),
    /// Includes things like `null`, `true`, `false`.
    /// Should probably treat these as literals, but we can change if we have a use case.
    Word(t::Word),
    PathVariant(PathPattern),
    Nested(NestedPattern),
}

impl UnionPatternMember {
    pub fn take(it: &mut SyntaxNodeIter) -> Result<Self, StrongAstError> {
        let first = it.expect_next("a literal or WORD")?;
        let first = match first.kind() {
            SyntaxKind::WORD => t::Word::new_from_span(first.text_range()),
            SyntaxKind::INTEGER_LITERAL
            | SyntaxKind::FLOAT_LITERAL
            | SyntaxKind::STRING_LITERAL => {
                return Literal::from_cst(first).map(UnionPatternMember::Literal);
            }
            SyntaxKind::L_PAREN => {
                let open_paren = t::LParen::from_cst(first)?;
                let pattern = it.expect_parse()?;
                let close_paren = it.expect_parse()?;
                return Ok(UnionPatternMember::Nested(NestedPattern {
                    open_paren,
                    pattern: Box::new(pattern),
                    close_paren,
                }));
            }
            found => {
                return Err(StrongAstError::UnexpectedKindDesc {
                    expected_desc: "literal or WORD".into(),
                    found,
                    at: first.text_range(),
                });
            }
        };

        let mut rest = Vec::new();
        while let Some(dot) = it.next_if_kind(SyntaxKind::DOT) {
            let dot = t::Dot::from_cst(dot)?;
            let word: t::Word = it.expect_parse()?;
            rest.push((dot, word));
        }

        if rest.is_empty() {
            Ok(UnionPatternMember::Word(first))
        } else {
            Ok(UnionPatternMember::PathVariant(PathPattern { first, rest }))
        }
    }
}

impl From<UnionPatternMember> for MatchPattern {
    fn from(member: UnionPatternMember) -> Self {
        match member {
            UnionPatternMember::Literal(lit) => MatchPattern::Literal(lit),
            UnionPatternMember::PathVariant(variant) => MatchPattern::Path(variant),
            UnionPatternMember::Word(word) => MatchPattern::Binding(BindingPattern {
                name: word,
                ty: None,
            }),
            UnionPatternMember::Nested(nested) => MatchPattern::Nested(nested),
        }
    }
}

impl Printable for UnionPatternMember {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            UnionPatternMember::Literal(lit) => printer.print(lit, shape),
            UnionPatternMember::Word(word) => {
                printer.print_raw_token(word);
                PrintInfo::default_single_line()
            }
            UnionPatternMember::PathVariant(variant) => variant.print(shape, printer),
            UnionPatternMember::Nested(nested) => nested.print(shape, printer),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            UnionPatternMember::Literal(lit) => lit.leftmost_token(),
            UnionPatternMember::Word(word) => word.span(),
            UnionPatternMember::PathVariant(variant) => variant.leftmost_token(),
            UnionPatternMember::Nested(nested) => nested.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            UnionPatternMember::Literal(lit) => lit.rightmost_token(),
            UnionPatternMember::Word(word) => word.span(),
            UnionPatternMember::PathVariant(variant) => variant.rightmost_token(),
            UnionPatternMember::Nested(nested) => nested.rightmost_token(),
        }
    }
}

#[derive(Debug)]
pub struct NestedPattern {
    pub open_paren: t::LParen,
    pub pattern: Box<MatchPattern>,
    pub close_paren: t::RParen,
}

impl PrintMultiLine for NestedPattern {
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_indent = shape.indent + printer.config.indent_width;
        printer.print_raw_token(&self.open_paren);
        printer.print_trivia_all_trailing_for(self.open_paren.span());
        printer.print_newline();

        printer.print_standalone_with_trivia(&*self.pattern, inner_indent);

        printer.print_trivia_all_leading_with_newline_for(self.close_paren.span(), inner_indent);
        printer.print_newline();
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_paren);
        PrintInfo::default_multi_lined()
    }
}

impl NestedPattern {
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the nested pattern on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.open_paren);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_paren.span());
        printer.try_print_trivia_single_line_squished(open_trailing)?;

        let (pattern_leading, pattern_trailing) = printer.trivia.get_for_element(&*self.pattern);
        printer.try_print_trivia_single_line_squished(pattern_leading)?;
        if printer
            .print(&*self.pattern, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        printer.try_print_trivia_single_line_squished(pattern_trailing)?;

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

impl Printable for NestedPattern {
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
