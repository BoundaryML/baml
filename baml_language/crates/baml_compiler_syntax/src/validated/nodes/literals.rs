use super::super::{FromCST, StrongAstError, tokens as t};
use crate::{SyntaxElement, SyntaxKind};

#[derive(Debug)]
pub enum Literal {
    String(t::QuotedString),
    Bigint(t::BigintLiteral),
    Integer(t::IntegerLiteral),
    Float(t::FloatLiteral),
    Keyword(t::KeywordLiteral),
}

impl FromCST for Literal {
    fn from_cst(element: SyntaxElement) -> Result<Self, StrongAstError> {
        match element.kind() {
            SyntaxKind::STRING_LITERAL => t::QuotedString::from_cst(element).map(Self::String),
            SyntaxKind::BIGINT_LITERAL => t::BigintLiteral::from_cst(element).map(Self::Bigint),
            SyntaxKind::INTEGER_LITERAL => t::IntegerLiteral::from_cst(element).map(Self::Integer),
            SyntaxKind::FLOAT_LITERAL => t::FloatLiteral::from_cst(element).map(Self::Float),
            SyntaxKind::KW_TRUE | SyntaxKind::KW_FALSE | SyntaxKind::KW_NULL => {
                t::KeywordLiteral::from_cst(element).map(Self::Keyword)
            }
            found => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "a literal".into(),
                found,
                at: element.text_range(),
            }),
        }
    }
}
