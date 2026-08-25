use super::super::{
    FromCST, KnownKind, StrongAstError, SyntaxNodeIter, ValidatedToken as _, tokens as t,
};
use crate::{SyntaxElement, SyntaxKind, SyntaxNodeExt as _, TextRange};

#[derive(Debug)]
pub struct BlockAttribute {
    pub atat: t::AtAt,
    pub name: AttributeName,
    pub args: Option<AttributeArgs>,
}

impl FromCST for BlockAttribute {
    fn from_cst(element: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(element)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::BLOCK_ATTRIBUTE)?;
        let mut elements = SyntaxNodeIter::new(&node);
        let atat = elements.expect_parse()?;
        let name = AttributeName::take(&mut elements)?;
        let args = elements.next().map(AttributeArgs::from_cst).transpose()?;
        elements.expect_end()?;
        Ok(Self { atat, name, args })
    }
}

impl KnownKind for BlockAttribute {
    fn kind() -> SyntaxKind {
        SyntaxKind::BLOCK_ATTRIBUTE
    }
}

impl BlockAttribute {
    pub fn name_parts_str<'source>(
        &self,
        input: &'source str,
    ) -> impl Iterator<Item = &'source str> {
        std::iter::once(&self.name.first)
            .chain(self.name.rest.iter().map(|(_, part)| part))
            .map(|part| match part {
                AttributeNamePart::Word(word) => &input[word.span()],
                AttributeNamePart::Keyword(range) => &input[*range],
            })
    }
}

#[derive(Debug)]
pub struct Attribute {
    pub at: t::At,
    pub name: AttributeName,
    pub args: Option<AttributeArgs>,
}

impl FromCST for Attribute {
    fn from_cst(element: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(element)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::ATTRIBUTE)?;
        let mut elements = SyntaxNodeIter::new(&node);
        let at = elements.expect_parse()?;
        let first = AttributeNamePart::from_cst(elements.expect_next("attribute name part")?)?;
        let mut rest = Vec::new();
        let args = loop {
            let Some(element) = elements.next() else {
                break None;
            };
            match element.kind() {
                SyntaxKind::DOT => {
                    let dot = StrongAstError::assert_is_token(element)?;
                    let part =
                        AttributeNamePart::from_cst(elements.expect_next("attribute name part")?)?;
                    rest.push((t::Dot::new_from_span(dot.text_range()), part));
                }
                SyntaxKind::ATTRIBUTE_ARGS => break Some(AttributeArgs::from_cst(element)?),
                found => {
                    return Err(StrongAstError::UnexpectedKindDesc {
                        expected_desc: "DOT or ATTRIBUTE_ARGS".into(),
                        found,
                        at: element.text_range(),
                    });
                }
            }
        };
        elements.expect_end()?;
        Ok(Self {
            at,
            name: AttributeName { first, rest },
            args,
        })
    }
}

impl KnownKind for Attribute {
    fn kind() -> SyntaxKind {
        SyntaxKind::ATTRIBUTE
    }
}

impl Attribute {
    #[must_use]
    pub fn non_wrappable_len(&self) -> usize {
        1 + self.name.first.len()
            + self
                .name
                .rest
                .iter()
                .map(|(_, part)| 1 + part.len())
                .sum::<usize>()
            + usize::from(self.args.is_some())
    }
}

#[derive(Debug)]
pub enum AttributeNamePart {
    Word(t::Word),
    Keyword(TextRange),
}

impl FromCST for AttributeNamePart {
    fn from_cst(element: SyntaxElement) -> Result<Self, StrongAstError> {
        let token = StrongAstError::assert_is_token(element)?;
        match token.kind() {
            SyntaxKind::WORD => Ok(Self::Word(t::Word::new_from_span(token.text_range()))),
            keyword if keyword.is_keyword() => Ok(Self::Keyword(token.text_range())),
            found => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "keyword or word".into(),
                found,
                at: token.text_range(),
            }),
        }
    }
}

impl AttributeNamePart {
    #[allow(
        clippy::len_without_is_empty,
        reason = "attribute name parts cannot be empty"
    )]
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::Word(word) => word.span().len().into(),
            Self::Keyword(range) => range.len().into(),
        }
    }
}

#[derive(Debug)]
pub struct AttributeName {
    pub first: AttributeNamePart,
    pub rest: Vec<(t::Dot, AttributeNamePart)>,
}

impl AttributeName {
    pub fn take(elements: &mut SyntaxNodeIter) -> Result<Self, StrongAstError> {
        let first = elements.expect_token("attribute name part")?;
        let first = AttributeNamePart::from_cst(SyntaxElement::Token(first))?;
        let mut rest = Vec::new();
        while let Some(dot) = elements.next_if_kind(SyntaxKind::DOT) {
            let dot = StrongAstError::assert_is_token(dot)?;
            let part = elements.expect_token("attribute name part")?;
            let part = AttributeNamePart::from_cst(SyntaxElement::Token(part))?;
            rest.push((t::Dot::new_from_span(dot.text_range()), part));
        }
        Ok(Self { first, rest })
    }
}

#[derive(Debug)]
pub struct AttributeArgs {
    pub open_paren: t::LParen,
    pub args: Vec<(AttributeArg, Option<t::Comma>)>,
    pub close_paren: t::RParen,
}

impl FromCST for AttributeArgs {
    fn from_cst(element: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(element)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::ATTRIBUTE_ARGS)?;
        let mut elements = SyntaxNodeIter::new(&node);
        let open_paren = elements.expect_parse()?;
        let mut args = Vec::new();
        let close_paren = loop {
            let Some(element) = elements.next() else {
                return Err(StrongAstError::missing(
                    SyntaxKind::R_PAREN,
                    elements.parent,
                ));
            };
            if element.kind() == SyntaxKind::R_PAREN {
                break t::RParen::from_cst(element)?;
            }
            let argument = AttributeArg::from_cst(element)?;
            let comma = elements
                .next_if_kind(SyntaxKind::COMMA)
                .map(t::Comma::from_cst)
                .transpose()?;
            args.push((argument, comma));
        };
        elements.expect_end()?;
        Ok(Self {
            open_paren,
            args,
            close_paren,
        })
    }
}

impl KnownKind for AttributeArgs {
    fn kind() -> SyntaxKind {
        SyntaxKind::ATTRIBUTE_ARGS
    }
}

#[derive(Debug)]
pub enum AttributeArg {
    QuotedString(t::QuotedString),
    RawString(t::RawString),
    Backtick(t::BacktickString),
    AttrExpr(TextRange),
    UnquotedString(t::Word),
}

impl FromCST for AttributeArg {
    fn from_cst(element: SyntaxElement) -> Result<Self, StrongAstError> {
        match element.kind() {
            SyntaxKind::STRING_LITERAL => {
                t::QuotedString::from_cst(element).map(Self::QuotedString)
            }
            SyntaxKind::RAW_STRING_LITERAL => t::RawString::from_cst(element).map(Self::RawString),
            SyntaxKind::BACKTICK_STRING_LITERAL => {
                t::BacktickString::from_cst(element).map(Self::Backtick)
            }
            SyntaxKind::EXPR => {
                let node = StrongAstError::assert_is_node(element)?;
                let start = node
                    .first_child_token_of_kind(SyntaxKind::L_BRACE)
                    .ok_or_else(|| {
                        StrongAstError::missing(SyntaxKind::L_BRACE, node.text_range())
                    })?;
                Ok(Self::AttrExpr(TextRange::new(
                    start.text_range().start(),
                    node.text_range().end(),
                )))
            }
            SyntaxKind::UNQUOTED_STRING => {
                let node = StrongAstError::assert_is_node(element)?;
                let mut elements = SyntaxNodeIter::new(&node);
                let word = elements.expect_parse()?;
                elements.expect_end()?;
                Ok(Self::UnquotedString(word))
            }
            found => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "attribute argument".into(),
                found,
                at: element.text_range(),
            }),
        }
    }
}
