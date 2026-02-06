//! Reference: [baml_compiler_syntax::ast::MatchPattern] and [baml_compiler_hir::body::Pattern]

use baml_compiler_syntax::{SyntaxElement, SyntaxKind, SyntaxNode};

use crate::ast::{FromCST, Literal, StrongAstError, SyntaxNodeIter, Type, tokens as t};

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

        // Look at first element to determine pattern type
        let Some(first_elem) = it.next() else {
            return Err(StrongAstError::missing_desc(
                "pattern content",
                node.text_range(),
            ));
        };

        match first_elem.kind() {
            // Literal patterns (string, integer, float)
            SyntaxKind::STRING_LITERAL => {
                let token = StrongAstError::assert_is_token(first_elem)?;
                it.expect_end()?;
                Ok(MatchPattern::Literal(Literal::String(t::QuotedString {
                    token_span: token.text_range(),
                })))
            }
            SyntaxKind::INTEGER_LITERAL => {
                let token = StrongAstError::assert_is_token(first_elem)?;
                it.expect_end()?;
                Ok(MatchPattern::Literal(Literal::Integer(t::IntegerLiteral {
                    token_span: token.text_range(),
                })))
            }
            SyntaxKind::FLOAT_LITERAL => {
                let token = StrongAstError::assert_is_token(first_elem)?;
                it.expect_end()?;
                Ok(MatchPattern::Literal(Literal::Float(t::FloatLiteral {
                    token_span: token.text_range(),
                })))
            }
            // Word can be binding or start of enum variant
            SyntaxKind::WORD => {
                let word_token = StrongAstError::assert_is_token(first_elem)?;
                let word = t::Word {
                    token_span: word_token.text_range(),
                };

                // Check next element to determine if it's binding or enum variant
                if let Some(next_elem) = it.next() {
                    match next_elem.kind() {
                        SyntaxKind::COLON => {
                            // Binding with type annotation: name : Type
                            let colon = StrongAstError::assert_is_token(next_elem)?;
                            let type_node = it.expect_node_of_kind(SyntaxKind::TYPE_EXPR)?;
                            let ty = Type::from_cst(SyntaxElement::Node(type_node))?;
                            it.expect_end()?;

                            Ok(MatchPattern::Binding(BindingPattern {
                                name: word,
                                ty: Some((t::Colon::new_from_span(colon.text_range()), ty)),
                            }))
                        }
                        SyntaxKind::DOT => {
                            // Enum variant: EnumName.VariantName
                            let dot = StrongAstError::assert_is_token(next_elem)?;
                            let variant = it.expect_token_of_kind(SyntaxKind::WORD)?;
                            it.expect_end()?;

                            Ok(MatchPattern::EnumVariant(EnumVariantPattern {
                                enum_name: word,
                                dot: t::Dot::new_from_span(dot.text_range()),
                                variant_name: t::Word {
                                    token_span: variant.text_range(),
                                },
                            }))
                        }
                        SyntaxKind::PIPE => {
                            // Union pattern: pattern | pattern | ...
                            // The first pattern is just a binding
                            let first_pattern = MatchPattern::Binding(BindingPattern {
                                name: word,
                                ty: None,
                            });

                            let mut rest = Vec::new();
                            let mut current_elem = next_elem;
                            loop {
                                let pipe = StrongAstError::assert_is_token(current_elem)?;
                                let next_pattern_node =
                                    it.expect_node_of_kind(SyntaxKind::MATCH_PATTERN)?;
                                let next_pattern =
                                    MatchPattern::from_cst(SyntaxElement::Node(next_pattern_node))?;
                                rest.push((
                                    t::Pipe::new_from_span(pipe.text_range()),
                                    next_pattern,
                                ));

                                if let Some(elem) = it.next() {
                                    if elem.kind() == SyntaxKind::PIPE {
                                        current_elem = elem;
                                    } else {
                                        return Err(StrongAstError::UnexpectedAdditionalElement {
                                            parent: node.text_range(),
                                            at: elem.text_range(),
                                        });
                                    }
                                } else {
                                    break;
                                }
                            }

                            it.expect_end()?;
                            Ok(MatchPattern::Union(UnionPattern {
                                first: Box::new(first_pattern),
                                rest,
                            }))
                        }
                        _ => Err(StrongAstError::UnexpectedKindDesc {
                            expected_desc: "COLON, DOT, or PIPE after WORD in pattern".into(),
                            found: next_elem.kind(),
                            at: next_elem.text_range(),
                        }),
                    }
                } else {
                    // Just a binding without type annotation
                    Ok(MatchPattern::Binding(BindingPattern {
                        name: word,
                        ty: None,
                    }))
                }
            }
            _ => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "STRING_LITERAL, INTEGER_LITERAL, FLOAT_LITERAL, or WORD".into(),
                found: first_elem.kind(),
                at: first_elem.text_range(),
            }),
        }
    }
}

#[derive(Debug)]
pub struct BindingPattern {
    pub name: t::Word,
    pub ty: Option<(t::Colon, Type)>,
}

#[derive(Debug)]
pub struct EnumVariantPattern {
    pub enum_name: t::Word,
    pub dot: t::Dot,
    pub variant_name: t::Word,
}

#[derive(Debug)]
pub struct UnionPattern {
    pub first: Box<MatchPattern>,
    pub rest: Vec<(t::Pipe, MatchPattern)>,
}

#[derive(Debug)]
pub enum UnionPatternMember {
    Literal(Literal),
    /// Includes things like `null`, `true`, `false`.
    /// Should probably treat these as literals, but we can change if we have a use case.
    Word(t::Word),
    EnumVariant(EnumVariantPattern),
}
