use baml_compiler_syntax::{SyntaxElement, SyntaxKind};
use rowan::TextRange;

use crate::ast::{FromCST, StrongAstError, SyntaxNodeIter, tokens as t};
use crate::printer::*;

/// Corresponds to a [`SyntaxKind::BLOCK_ATTRIBUTE`] node.
#[derive(Debug)]
pub struct BlockAttribute {
    pub atat: t::AtAt,
    pub name: AttributeName,
    pub args: Option<AttributeArgs>,
}

impl FromCST for BlockAttribute {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::BLOCK_ATTRIBUTE)?;

        let mut it = SyntaxNodeIter::new(node);

        // @@
        let atat = it.expect_token_of_kind(SyntaxKind::AT_AT)?;

        // name (can have dots like @stream.done)
        let name = AttributeName::take(&mut it)?;

        let args = it.next().map(AttributeArgs::from_cst).transpose()?;

        Ok(BlockAttribute {
            atat: t::AtAt::new_from_span(atat.text_range()),
            name,
            args,
        })
    }
}

impl Printable for BlockAttribute {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        printer.print_raw_token(&self.atat);
        multi_lined |= printer.print(&self.name, shape.clone()).multi_lined;
        if let Some(args) = &self.args {
            multi_lined |= printer.print(args, shape).multi_lined;
        }
        PrintInfo { multi_lined }
    }
}

/// Corresponds to a [`SyntaxKind::ATTRIBUTE`] node.
#[derive(Debug)]
pub struct Attribute {
    pub at: t::At,
    pub name: AttributeName,
    pub args: Option<AttributeArgs>,
}

impl FromCST for Attribute {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::ATTRIBUTE)?;

        let mut it = SyntaxNodeIter::new(node);

        // @
        let at = it.expect_token_of_kind(SyntaxKind::AT)?;

        // name (can have dots like @stream.done)
        let name_first = it.expect_next("attribute name part")?;
        let name_first = AttributeNamePart::from_cst(name_first)?;
        let mut name_rest = Vec::new();
        let args = loop {
            let Some(elem) = it.next() else {
                break None;
            };
            match elem.kind() {
                SyntaxKind::DOT => {
                    let dot = StrongAstError::assert_is_token(elem)?;
                    let name = it.expect_next("attribute name part")?;
                    let name = AttributeNamePart::from_cst(name)?;
                    name_rest.push((t::Dot::new_from_span(dot.text_range()), name));
                }
                SyntaxKind::ATTRIBUTE_ARGS => {
                    let args = AttributeArgs::from_cst(elem)?;
                    break Some(args);
                }
                _ => {
                    return Err(StrongAstError::UnexpectedKindDesc {
                        expected_desc: "DOT or ATTRIBUTE_ARGS".into(),
                        found: elem.kind(),
                        at: elem.text_range(),
                    });
                }
            }
        };

        let name = AttributeName {
            first: name_first,
            rest: name_rest,
        };

        Ok(Attribute {
            at: t::At::new_from_span(at.text_range()),
            name,
            args,
        })
    }
}

impl Printable for Attribute {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.at);
        printer.print(&self.name, shape.clone());
        if let Some(args) = &self.args {
            printer.print(args, shape);
        }
        PrintInfo::default_single_line()
    }
}

/// Attribute names are not normal paths: they may contain keywords.
#[derive(Debug)]
pub enum AttributeNamePart {
    Word(t::Word),
    Keyword(TextRange),
}

impl FromCST for AttributeNamePart {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let token = StrongAstError::assert_is_token(elem)?;
        match token.kind() {
            SyntaxKind::WORD => Ok(AttributeNamePart::Word(t::Word::new_from_span(
                token.text_range(),
            ))),
            keyword if keyword.is_keyword() => Ok(AttributeNamePart::Keyword(token.text_range())),
            _ => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "KEYWORD or WORD".into(),
                found: token.kind(),
                at: token.text_range(),
            }),
        }
    }
}

impl Printable for AttributeNamePart {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            AttributeNamePart::Word(word) => printer.print_raw_token(word),
            AttributeNamePart::Keyword(range) => printer.print_input_range(*range),
        }
        PrintInfo::default_single_line()
    }
}

/// Attribute names are not normal paths: they may contain keywords.
#[derive(Debug)]
pub struct AttributeName {
    pub first: AttributeNamePart,
    pub rest: Vec<(t::Dot, AttributeNamePart)>,
}

impl AttributeName {
    pub fn take(it: &mut SyntaxNodeIter) -> Result<Self, StrongAstError> {
        let first = it.expect_token("attribute name part")?;
        let first = AttributeNamePart::from_cst(SyntaxElement::Token(first))?;

        let mut rest = Vec::new();
        while let Some(dot) = it.next_if(|elem| elem.kind() == SyntaxKind::DOT) {
            let dot_token = StrongAstError::assert_is_token(dot)?;
            let part = it.expect_token("attribute name part")?;
            let part = AttributeNamePart::from_cst(SyntaxElement::Token(part))?;
            rest.push((t::Dot::new_from_span(dot_token.text_range()), part));
        }

        Ok(AttributeName { first, rest })
    }
}

impl Printable for AttributeName {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print(&self.first, shape.clone());
        for (dot, part) in &self.rest {
            printer.print_raw_token(dot);
            printer.print(part, shape.clone());
        }
        PrintInfo::default_single_line()
    }
}

/// Corresponds to a [`SyntaxKind::ATTRIBUTE_ARGS`] node.
#[derive(Debug)]
pub struct AttributeArgs {
    pub todo: TextRange, // TODO
}
impl FromCST for AttributeArgs {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::ATTRIBUTE_ARGS)?;

        let todo = node.text_range();

        Ok(AttributeArgs { todo })
    }
}

impl Printable for AttributeArgs {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_input_range(self.todo);
        PrintInfo::default_single_line()
    }
}
