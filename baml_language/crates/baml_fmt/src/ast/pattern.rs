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
    EnumVariant(EnumVariantPattern),
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
                UnionPatternMember::EnumVariant(variant) => MatchPattern::EnumVariant(variant),
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
            MatchPattern::EnumVariant(variant) => variant.print(shape, printer),
            MatchPattern::Union(union) => union.print(shape, printer),
            MatchPattern::Nested(nested) => nested.print(shape, printer),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            MatchPattern::Literal(lit) => lit.leftmost_token(),
            MatchPattern::Binding(binding) => binding.leftmost_token(),
            MatchPattern::EnumVariant(variant) => variant.leftmost_token(),
            MatchPattern::Union(union) => union.leftmost_token(),
            MatchPattern::Nested(nested) => nested.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            MatchPattern::Literal(lit) => lit.rightmost_token(),
            MatchPattern::Binding(binding) => binding.rightmost_token(),
            MatchPattern::EnumVariant(variant) => variant.rightmost_token(),
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
            printer.print_str(" ");
            let new_overhead = usize::from(self.name.span().len() + colon.span().len()) + 1;
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
pub struct EnumVariantPattern {
    pub enum_name: t::Word,
    pub dot: t::Dot,
    pub variant_name: t::Word,
}

impl Printable for EnumVariantPattern {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.enum_name);
        printer.print_raw_token(&self.dot);
        printer.print_raw_token(&self.variant_name);
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.enum_name.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.variant_name.span()
    }
}

#[derive(Debug)]
pub struct UnionPattern {
    pub first: Box<UnionPatternMember>,
    pub rest: Vec<(t::Pipe, UnionPatternMember)>,
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
        printer.print_trivia_all_trailing_for(self.first.rightmost_token());
        for (pipe, pattern) in &self.rest {
            info.multi_lined = true;
            printer.print_newline();
            printer.print_spaces(shape.indent + printer.config.indent_width);
            printer.print_raw_token(pipe);
            printer.print_str(" ");
            printer.print(pattern, shape.clone());
            printer.print_trivia_all_trailing_for(pattern.rightmost_token());
        }
        info
    }
}

impl Printable for UnionPattern {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        // Check if trailing trivia on any member forces multi-line
        let (_, first_trailing) = printer
            .trivia
            .get_for_range_split(self.first.rightmost_token());
        let mut has_line_trivia = first_trailing
            .iter()
            .any(|t| t.single_line_len(printer.input).is_none());

        if !has_line_trivia {
            for (_, pattern) in &self.rest {
                let (_, trailing) = printer
                    .trivia
                    .get_for_range_split(pattern.rightmost_token());
                if trailing
                    .iter()
                    .any(|t| t.single_line_len(printer.input).is_none())
                {
                    has_line_trivia = true;
                    break;
                }
            }
        }

        if has_line_trivia {
            return Self::print_multi_line(self, shape, printer);
        }

        let mut single_line_printer =
            Printer::new_empty(printer.input, printer.config, printer.trivia);
        let mut multi_lined = false;
        multi_lined |= single_line_printer
            .print(&*self.first, shape.clone())
            .multi_lined;
        for (pipe, pattern) in &self.rest {
            if multi_lined || single_line_printer.output.len() > shape.width {
                return Self::print_multi_line(self, shape, printer);
            }
            single_line_printer.print_str(" ");
            single_line_printer.print_raw_token(pipe);
            single_line_printer.print_str(" ");
            multi_lined |= single_line_printer
                .print(pattern, shape.clone())
                .multi_lined;
        }
        if multi_lined || single_line_printer.output.len() > shape.width {
            return Self::print_multi_line(self, shape, printer);
        }

        printer.append_from_printer(single_line_printer);
        PrintInfo::default_single_line()
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
    EnumVariant(EnumVariantPattern),
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

        if let Some(dot) = it.next_if_kind(SyntaxKind::DOT) {
            let dot = t::Dot::from_cst(dot)?;
            let variant_name = it.expect_parse()?;
            Ok(UnionPatternMember::EnumVariant(EnumVariantPattern {
                enum_name: first,
                dot,
                variant_name,
            }))
        } else {
            Ok(UnionPatternMember::Word(first))
        }
    }
}

impl From<UnionPatternMember> for MatchPattern {
    fn from(member: UnionPatternMember) -> Self {
        match member {
            UnionPatternMember::Literal(lit) => MatchPattern::Literal(lit),
            UnionPatternMember::EnumVariant(variant) => MatchPattern::EnumVariant(variant),
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
            UnionPatternMember::EnumVariant(variant) => variant.print(shape, printer),
            UnionPatternMember::Nested(nested) => nested.print(shape, printer),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            UnionPatternMember::Literal(lit) => lit.leftmost_token(),
            UnionPatternMember::Word(word) => word.span(),
            UnionPatternMember::EnumVariant(variant) => variant.leftmost_token(),
            UnionPatternMember::Nested(nested) => nested.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            UnionPatternMember::Literal(lit) => lit.rightmost_token(),
            UnionPatternMember::Word(word) => word.span(),
            UnionPatternMember::EnumVariant(variant) => variant.rightmost_token(),
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

impl Printable for NestedPattern {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut inner_printer = Printer::new_empty(printer.input, printer.config, printer.trivia);
        let inner_shape = Shape {
            width: shape.width.saturating_sub(2),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };
        let inner_info = inner_printer.print(&*self.pattern, inner_shape);
        if inner_info.multi_lined {
            return self.print_multi_line(shape, printer);
        }

        // Check trivia between parens and inner pattern
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_paren.span());
        let (pattern_leading, _) = printer
            .trivia
            .get_for_range_split(self.pattern.leftmost_token());
        let (_, pattern_trailing) = printer
            .trivia
            .get_for_range_split(self.pattern.rightmost_token());
        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_paren.span());
        let single_line_len: usize = open_trailing
            .iter()
            .chain(pattern_leading)
            .chain(pattern_trailing)
            .chain(close_leading)
            .map(|t| t.single_line_len(printer.input))
            .sum::<Option<usize>>()
            .map_or(usize::MAX, |sum| {
                sum + inner_printer.len() + const { "()".len() }
            });

        if single_line_len > shape.width {
            self.print_multi_line(shape, printer)
        } else {
            printer.print_raw_token(&self.open_paren);
            for t in open_trailing {
                printer.print_trivia(t);
            }
            for t in pattern_leading {
                if t.is_comment() {
                    printer.print_trivia(t);
                }
            }
            printer.append_from_printer(inner_printer);
            for t in pattern_trailing {
                printer.print_trivia(t);
            }
            for t in close_leading {
                if t.is_comment() {
                    printer.print_trivia(t);
                }
            }
            printer.print_raw_token(&self.close_paren);
            PrintInfo::default_single_line()
        }
    }
    fn leftmost_token(&self) -> TextRange {
        self.open_paren.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_paren.span()
    }
}
