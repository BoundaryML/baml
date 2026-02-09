//! Reference: [baml_compiler_syntax::ast::MatchPattern] and [baml_compiler_hir::body::Pattern]

use baml_compiler_syntax::{SyntaxElement, SyntaxKind};

use crate::ast::{FromCST, Literal, StrongAstError, SyntaxNodeIter, tokens as t};
use crate::printer::*;

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
}

impl FromCST for MatchPattern {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::MATCH_PATTERN)?;

        let mut it = SyntaxNodeIter::new(node.clone());

        // if first elem is a word, it could also be a binding, not a pattern
        let mut first_elem = UnionPatternMember::take(&mut it)?;

        let binding = if it.peek().is_none() {
            return Ok(first_elem.into());
        } else if let Some(colon) = it.next_if(|elem| elem.kind() == SyntaxKind::COLON) {
            let colon = StrongAstError::assert_is_token(colon)?;
            let UnionPatternMember::Word(binding_name) = first_elem else {
                return Err(StrongAstError::UnexpectedKindDesc {
                    expected_desc: "PIPE".into(),
                    found: colon.kind(),
                    at: colon.text_range(),
                });
            };
            first_elem = UnionPatternMember::take(&mut it)?;
            Some((binding_name, t::Colon::new_from_span(colon.text_range())))
        } else {
            None
        };

        let mut rest = Vec::new();
        while let Some(pipe) = it.next() {
            let pipe = StrongAstError::assert_is_token(pipe)?;
            StrongAstError::assert_kind_token(&pipe, SyntaxKind::PIPE)?;

            let next = UnionPatternMember::take(&mut it)?;

            rest.push((t::Pipe::new_from_span(pipe.text_range()), next));
        }

        let ty = if rest.is_empty() {
            match first_elem {
                UnionPatternMember::Literal(lit) => BindingPatternPattern::Literal(lit),
                UnionPatternMember::EnumVariant(variant) => {
                    BindingPatternPattern::EnumVariant(variant)
                }
                UnionPatternMember::Word(word) => BindingPatternPattern::Word(word),
            }
        } else {
            BindingPatternPattern::Union(UnionPattern {
                first: Box::new(first_elem),
                rest,
            })
        };

        if let Some((binding_name, colon)) = binding {
            Ok(MatchPattern::Binding(BindingPattern {
                name: binding_name,
                ty: Some((colon, ty)),
            }))
        } else {
            Ok(ty.into())
        }
    }
}

impl Printable for MatchPattern {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            MatchPattern::Literal(lit) => printer.print(lit, shape),
            MatchPattern::Binding(binding) => binding.print(shape, printer),
            MatchPattern::EnumVariant(variant) => variant.print(shape, printer),
            MatchPattern::Union(union) => union.print(shape, printer),
        }
    }
}

#[derive(Debug)]
pub struct BindingPattern {
    pub name: t::Word,
    pub ty: Option<(t::Colon, BindingPatternPattern)>,
}

impl Printable for BindingPattern {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.name);
        if let Some((colon, ty)) = &self.ty {
            printer.print_raw_token(colon);
            printer.print_str(" ");
            printer.print(ty, shape);
        }
        PrintInfo::default_single_line()
    }
}

#[derive(Debug)]
pub enum BindingPatternPattern {
    Literal(Literal),
    Word(t::Word),
    EnumVariant(EnumVariantPattern),
    Union(UnionPattern),
}

impl From<BindingPatternPattern> for MatchPattern {
    fn from(pattern: BindingPatternPattern) -> Self {
        match pattern {
            BindingPatternPattern::Literal(lit) => MatchPattern::Literal(lit),
            BindingPatternPattern::Word(word) => MatchPattern::Binding(BindingPattern {
                name: word,
                ty: None,
            }),
            BindingPatternPattern::EnumVariant(variant) => MatchPattern::EnumVariant(variant),
            BindingPatternPattern::Union(union) => MatchPattern::Union(union),
        }
    }
}

impl Printable for BindingPatternPattern {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            BindingPatternPattern::Literal(lit) => printer.print(lit, shape),
            BindingPatternPattern::Word(word) => {
                printer.print_raw_token(word);
                PrintInfo::default_single_line()
            }
            BindingPatternPattern::EnumVariant(variant) => variant.print(shape, printer),
            BindingPatternPattern::Union(union) => union.print(shape, printer),
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
}

#[derive(Debug)]
pub struct UnionPattern {
    pub first: Box<UnionPatternMember>,
    pub rest: Vec<(t::Pipe, UnionPatternMember)>,
}

impl Printable for UnionPattern {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print(&*self.first, shape.clone());

        for (pipe, pattern) in &self.rest {
            printer.print_str(" ");
            printer.print_raw_token(pipe);
            printer.print_str(" ");
            printer.print(pattern, shape.clone());
        }
        PrintInfo::default_single_line()
    }
}

#[derive(Debug)]
pub enum UnionPatternMember {
    Literal(Literal),
    /// Includes things like `null`, `true`, `false`.
    /// Should probably treat these as literals, but we can change if we have a use case.
    Word(t::Word),
    EnumVariant(EnumVariantPattern),
}

impl UnionPatternMember {
    pub fn take(it: &mut SyntaxNodeIter) -> Result<Self, StrongAstError> {
        let first = it.expect_next("a literal or WORD")?;
        let first = match first.kind() {
            SyntaxKind::WORD => t::Word::new_from_span(first.text_range()),
            SyntaxKind::INTEGER_LITERAL => {
                let token = StrongAstError::assert_is_token(first.clone())?;
                return Ok(UnionPatternMember::Literal(Literal::Integer(
                    t::IntegerLiteral::new_from_span(token.text_range()),
                )));
            }
            SyntaxKind::FLOAT_LITERAL => {
                let token = StrongAstError::assert_is_token(first.clone())?;
                return Ok(UnionPatternMember::Literal(Literal::Float(
                    t::FloatLiteral::new_from_span(token.text_range()),
                )));
            }
            SyntaxKind::STRING_LITERAL => {
                let token = StrongAstError::assert_is_token(first.clone())?;
                return Ok(UnionPatternMember::Literal(Literal::String(
                    t::QuotedString::new_from_span(token.text_range()),
                )));
            }
            found => {
                return Err(StrongAstError::UnexpectedKindDesc {
                    expected_desc: "literal or WORD".into(),
                    found,
                    at: first.text_range(),
                });
            }
        };

        if let Some(dot) = it.next_if(|elem| elem.kind() == SyntaxKind::DOT) {
            let dot = StrongAstError::assert_is_token(dot)?;
            let word = it.expect_token_of_kind(SyntaxKind::WORD)?;
            Ok(UnionPatternMember::EnumVariant(EnumVariantPattern {
                enum_name: first,
                dot: t::Dot::new_from_span(dot.text_range()),
                variant_name: t::Word::new_from_span(word.text_range()),
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
        }
    }
}
