use baml_db::baml_compiler_syntax::{SyntaxElement, SyntaxKind};
use rowan::TextRange;

use super::{
    AssociatedTypeDecl, BlockAttribute, ClassField, ClassFieldDelimiter, FromCST, FunctionDecl,
    FunctionSignature, GenericParamList, Printable, StrongAstError, SyntaxNodeIter, Token, Type,
    declarations::{print_declaration_body, print_leading_attributes},
    tokens as t,
};
use crate::printer::{PrintInfo, Printer, Shape};

#[derive(Debug)]
pub struct InterfaceDecl {
    pub attributes: Vec<BlockAttribute>,
    pub keyword: t::Interface,
    pub name: t::Word,
    pub generic_params: Option<GenericParamList>,
    pub requires: Option<RequiresClause>,
    pub open_brace: t::LBrace,
    pub items: Vec<InterfaceItem>,
    pub close_brace: t::RBrace,
}

impl FromCST for InterfaceDecl {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::INTERFACE_DEF)?;
        let mut it = SyntaxNodeIter::new(&node);
        let mut attributes = Vec::new();
        while let Some(elem) = it.next_if_kind(SyntaxKind::BLOCK_ATTRIBUTE) {
            attributes.push(BlockAttribute::from_cst(elem)?);
        }
        let keyword = it.expect_parse()?;
        let name = it.expect_parse()?;
        let generic_params = it
            .next_if_kind(SyntaxKind::GENERIC_PARAM_LIST)
            .map(GenericParamList::from_cst)
            .transpose()?;
        let requires = it
            .next_if_kind(SyntaxKind::REQUIRES_CLAUSE)
            .map(RequiresClause::from_cst)
            .transpose()?;
        let open_brace = it.expect_parse()?;
        let mut items = Vec::new();
        let close_brace = loop {
            let elem = it.expect_next("interface member or closing brace")?;
            let item = match elem.kind() {
                SyntaxKind::FIELD => InterfaceItem::Field(
                    ClassField::from_cst(elem)?,
                    ClassFieldDelimiter::take(&mut it)?,
                ),
                SyntaxKind::ASSOCIATED_TYPE_DECL => InterfaceItem::AssociatedType(
                    AssociatedTypeDecl::from_cst(elem)?,
                    ClassFieldDelimiter::take(&mut it)?,
                ),
                SyntaxKind::METHOD_SIG => InterfaceItem::RequiredMethod(
                    FunctionSignature::from_cst(elem)?,
                    it.next_if_kind(SyntaxKind::SEMICOLON)
                        .map(t::Semicolon::from_cst)
                        .transpose()?,
                ),
                SyntaxKind::FUNCTION_DEF => {
                    InterfaceItem::DefaultMethod(FunctionDecl::from_cst(elem)?)
                }
                SyntaxKind::BLOCK_ATTRIBUTE => {
                    InterfaceItem::Attribute(BlockAttribute::from_cst(elem)?)
                }
                SyntaxKind::R_BRACE => break t::RBrace::from_cst(elem)?,
                found => {
                    return Err(StrongAstError::UnexpectedKindDesc {
                        expected_desc: "interface member or closing brace".into(),
                        found,
                        at: elem.text_range(),
                    });
                }
            };
            items.push(item);
        };
        it.expect_end()?;
        Ok(Self {
            attributes,
            keyword,
            name,
            generic_params,
            requires,
            open_brace,
            items,
            close_brace,
        })
    }
}

impl Printable for InterfaceDecl {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        print_leading_attributes(&self.attributes, self.keyword.span(), shape.indent, printer);
        let continuation = shape.indent + printer.config.indent_width;
        printer.print_raw_token(&self.keyword);
        printer.print_separator(
            self.keyword.span(),
            Some(self.name.span()),
            continuation,
            " ",
        );
        printer.print_raw_token(&self.name);
        let mut previous = self.name.span();
        if let Some(params) = &self.generic_params {
            printer.print_separator(previous, Some(params.leftmost_token()), continuation, "");
            printer.print(params, shape.clone());
            previous = params.rightmost_token();
        }
        if let Some(requires) = &self.requires {
            printer.print_separator(previous, Some(requires.leftmost_token()), continuation, " ");
            printer.print(requires, shape.clone());
            previous = requires.rightmost_token();
        }
        printer.print_separator(previous, Some(self.open_brace.span()), shape.indent, " ");
        if self.items.is_empty()
            && printer.try_print_empty_braces(&self.open_brace, &self.close_brace)
        {
            return PrintInfo::default_single_line();
        }
        print_declaration_body(
            &self.open_brace,
            &self.items,
            &self.close_brace,
            &shape,
            printer,
        )
    }
    fn leftmost_token(&self) -> TextRange {
        self.attributes
            .first()
            .map_or(self.keyword.span(), Printable::leftmost_token)
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_brace.span()
    }
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum InterfaceItem {
    Field(ClassField, Option<ClassFieldDelimiter>),
    AssociatedType(AssociatedTypeDecl, Option<ClassFieldDelimiter>),
    RequiredMethod(FunctionSignature, Option<t::Semicolon>),
    DefaultMethod(FunctionDecl),
    Attribute(BlockAttribute),
}

impl Printable for InterfaceItem {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            Self::Field(field, delimiter) => {
                let info = field.print(shape, printer);
                ClassFieldDelimiter::print_comma(delimiter.as_ref(), printer);
                info
            }
            Self::AssociatedType(decl, delimiter) => {
                decl.print_delimited(delimiter.as_ref(), shape, printer)
            }
            Self::RequiredMethod(method, semicolon) => {
                let continuation = shape.indent + printer.config.indent_width;
                let info = method.print(shape, printer);
                printer.print_semicolon(
                    method.rightmost_token(),
                    semicolon.as_ref().map(Token::span),
                    continuation,
                );
                info
            }
            Self::DefaultMethod(method) => method.print(shape, printer),
            Self::Attribute(attr) => attr.print(shape, printer),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            Self::Field(field, _) => field.leftmost_token(),
            Self::AssociatedType(decl, _) => decl.leftmost_token(),
            Self::RequiredMethod(method, _) => method.leftmost_token(),
            Self::DefaultMethod(method) => method.leftmost_token(),
            Self::Attribute(attr) => attr.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            Self::Field(field, delimiter) => {
                ClassFieldDelimiter::rightmost(delimiter.as_ref(), || field.rightmost_token())
            }
            Self::AssociatedType(decl, delimiter) => {
                ClassFieldDelimiter::rightmost(delimiter.as_ref(), || decl.rightmost_token())
            }
            Self::RequiredMethod(method, semicolon) => semicolon
                .as_ref()
                .map_or_else(|| method.rightmost_token(), Token::span),
            Self::DefaultMethod(method) => method.rightmost_token(),
            Self::Attribute(attr) => attr.rightmost_token(),
        }
    }
}

#[derive(Debug)]
pub struct RequiresClause {
    pub keyword: t::Requires,
    pub targets: Vec<(Type, Option<t::Comma>)>,
}

impl FromCST for RequiresClause {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::REQUIRES_CLAUSE)?;
        let mut it = SyntaxNodeIter::new(&node);
        let keyword = it.expect_parse()?;
        let mut targets = Vec::new();
        loop {
            let ty = it.expect_parse()?;
            let comma = it
                .next_if_kind(SyntaxKind::COMMA)
                .map(t::Comma::from_cst)
                .transpose()?;
            let done = comma.is_none() || it.peek().is_none();
            targets.push((ty, comma));
            if done {
                break;
            }
        }
        it.expect_end()?;
        Ok(Self { keyword, targets })
    }
}

impl Printable for RequiresClause {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let continuation = shape.indent + printer.config.indent_width;
        printer.print_raw_token(&self.keyword);
        let mut previous = self.keyword.span();
        let mut multi_lined = false;
        for (ty, comma) in &self.targets {
            printer.print_separator(previous, Some(ty.leftmost_token()), continuation, " ");
            let mut candidate = printer.sub_printer();
            let info = candidate.print(ty, Shape::unlimited_single_line());
            if info.multi_lined
                || printer.current_line_len() + candidate.output.len() + 2
                    > printer.config.line_width
            {
                printer
                    .output
                    .truncate(printer.output.trim_end_matches(' ').len());
                printer.print_newline();
                printer.print_spaces(continuation);
                printer.print(
                    ty,
                    Shape::standalone(printer.config.line_width, continuation),
                );
                multi_lined = true;
            } else {
                printer.append_from_printer(candidate);
            }
            previous = ty.rightmost_token();
            if let Some(comma) = comma {
                printer.print_separator(previous, Some(comma.span()), continuation, "");
                printer.print_raw_token(comma);
                previous = comma.span();
            }
        }
        PrintInfo { multi_lined }
    }

    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.targets
            .last()
            .map_or(self.keyword.span(), |(ty, comma)| {
                comma
                    .as_ref()
                    .map_or_else(|| ty.rightmost_token(), Token::span)
            })
    }
}
