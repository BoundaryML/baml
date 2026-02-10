use baml_compiler_syntax::{SyntaxElement, SyntaxKind};

use crate::ast::{FromCST, StrongAstError, SyntaxNodeIter, Type, tokens as t};

#[derive(Debug)]
pub struct Generics {
    pub open_angle: t::Less,
    pub first: Box<Type>,
    pub rest: Vec<(t::Comma, Box<Type>)>,
    pub close_angle: t::Greater,
}

impl FromCST for Generics {
    fn from_cst(elem: baml_compiler_syntax::SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::TYPE_ARGS)?;

        let mut it = SyntaxNodeIter::new(node);

        let open_angle = it.expect_token_of_kind()?;

        let first = it.expect_node("type argument")?;
        let first = Type::from_cst(SyntaxElement::Node(first))?;

        let mut rest = Vec::new();

        let close_angle = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing(SyntaxKind::GREATER, it.parent));
            };
            match elem.kind() {
                SyntaxKind::COMMA => {
                    let comma = it.expect_token_of_kind()?;
                    let next = it.expect_node_of_kind(SyntaxKind::TYPE_EXPR)?;
                    let next = Type::from_cst(SyntaxElement::Node(next))?;
                    rest.push((comma, Box::new(next)));
                }
                SyntaxKind::GREATER => {
                    break t::Greater::from_cst(elem)?;
                }
                _ => {
                    return Err(StrongAstError::UnexpectedKindDesc {
                        expected_desc: "COMMA or GREATER".into(),
                        found: elem.kind(),
                        at: elem.text_range(),
                    });
                }
            }
        };

        it.expect_end()?;

        Ok(Generics {
            open_angle,
            first: Box::new(first),
            rest,
            close_angle,
        })
    }
}
