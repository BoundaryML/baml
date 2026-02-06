use baml_compiler_syntax::{SyntaxElement, SyntaxKind};

use crate::ast::{FromCST, StrongAstError, SyntaxNodeIter, Type, tokens as t};

#[derive(Debug)]
pub struct Generics {
    pub langle: t::Less,
    pub first: Box<Type>,
    pub rest: Vec<(t::Comma, Box<Type>)>,
    pub close: t::Greater,
}

impl FromCST for Generics {
    fn from_cst(elem: baml_compiler_syntax::SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::TYPE_ARGS)?;

        let mut it = SyntaxNodeIter::new(node);

        let langle = it.expect_token_of_kind(SyntaxKind::LESS)?;
        let langle = t::Less::new_from_span(langle.text_range());

        let first = it.expect_node("type argument")?;
        let first = Type::from_cst(SyntaxElement::Node(first))?;

        let mut rest = Vec::new();

        while let Some(elem) = it.next() {
            match elem.kind() {
                SyntaxKind::COMMA => {
                    let comma = it.expect_token_of_kind(SyntaxKind::COMMA)?;
                    let comma = t::Comma::new_from_span(comma.text_range());
                    let next = it.expect_node_of_kind(SyntaxKind::TYPE_EXPR)?;
                    let next = Type::from_cst(SyntaxElement::Node(next))?;
                    rest.push((comma, Box::new(next)));
                }
                SyntaxKind::GREATER => {
                    let token = StrongAstError::assert_is_token(elem)?;
                    it.expect_end()?;
                    return Ok(Generics {
                        langle,
                        first: Box::new(first),
                        rest,
                        close: t::Greater::new_from_span(token.text_range()),
                    });
                }
                _ => {
                    return Err(StrongAstError::UnexpectedKindDesc {
                        expected_desc: "COMMA or GREATER".into(),
                        found: elem.kind(),
                        at: elem.text_range(),
                    });
                }
            }
        }

        let close = it.expect_token_of_kind(SyntaxKind::GREATER)?;
        let close = t::Greater::new_from_span(close.text_range());

        it.expect_end()?;

        Ok(Generics {
            langle,
            first: Box::new(first),
            rest,
            close,
        })
    }
}
