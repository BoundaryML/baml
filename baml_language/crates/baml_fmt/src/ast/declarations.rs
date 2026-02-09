use baml_compiler_syntax::{SyntaxElement, SyntaxKind};
use rowan::TextRange;

use crate::ast::{
    Attribute, BlockAttribute, BlockExpr, Expression, FromCST, StrongAstError, SyntaxNodeIter,
    Type, tokens as t,
};
use crate::{ printer::*};

#[derive(Debug)]
pub enum TopLevelDeclaration {
    Function(FunctionDecl),
    Class(ClassDecl),
    Enum(EnumDecl),
    Client(ClientDecl),
    Test(TestDecl),
    RetryPolicy(TextRange),    // TODO
    TemplateString(TextRange), // TODO
    TypeAlias(TypeAliasDecl),
    Unknown(TextRange),
}

impl FromCST for TopLevelDeclaration {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let decl = match elem.kind() {
            SyntaxKind::FUNCTION_DEF => {
                TopLevelDeclaration::Function(FunctionDecl::from_cst(elem)?)
            }
            SyntaxKind::CLASS_DEF => TopLevelDeclaration::Class(ClassDecl::from_cst(elem)?),
            SyntaxKind::ENUM_DEF => TopLevelDeclaration::Enum(EnumDecl::from_cst(elem)?),
            SyntaxKind::CLIENT_DEF => TopLevelDeclaration::Client(ClientDecl::from_cst(elem)?),
            SyntaxKind::TEST_DEF => TopLevelDeclaration::Test(TestDecl::from_cst(elem)?),
            SyntaxKind::RETRY_POLICY_DEF => {
                TopLevelDeclaration::RetryPolicy(elem.text_range()) // TODO
            }
            SyntaxKind::TEMPLATE_STRING_DEF => {
                TopLevelDeclaration::TemplateString(elem.text_range()) // TODO
            }
            SyntaxKind::TYPE_ALIAS_DEF => {
                TopLevelDeclaration::TypeAlias(TypeAliasDecl::from_cst(elem)?)
            }
            _ => return Ok(TopLevelDeclaration::Unknown(elem.text_range())),
        };
        Ok(decl)
    }
}

impl Printable for TopLevelDeclaration {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            TopLevelDeclaration::Function(function_decl) => function_decl.print(shape, printer),
            TopLevelDeclaration::Class(class_decl) => class_decl.print(shape, printer),
            TopLevelDeclaration::Enum(enum_decl) => enum_decl.print(shape, printer),
            TopLevelDeclaration::Client(client_decl) => client_decl.print(shape, printer),
            TopLevelDeclaration::Test(test_decl) => test_decl.print(shape, printer),
            TopLevelDeclaration::RetryPolicy(range) => {
                printer.print_input_range(*range);
                PrintInfo::default_multi_lined()
            }
            TopLevelDeclaration::TemplateString(range) => {
                printer.print_input_range(*range);
                PrintInfo::default_multi_lined()
            }
            TopLevelDeclaration::TypeAlias(type_alias_decl) => {
                type_alias_decl.print(shape, printer)
            }
            TopLevelDeclaration::Unknown(range) => {
                printer.print_input_range(*range);
                PrintInfo::default_multi_lined()
            }
        }
    }
}

#[derive(Debug)]
pub struct FunctionDecl {
    pub keyword: t::Function,
    pub name: t::Word,
    pub params: FunctionParamList,
    pub arrow: t::Arrow,
    pub return_type: Type,
    pub body: FunctionDeclBody,
}
impl FromCST for FunctionDecl {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::FUNCTION_DEF)?;

        let mut it = SyntaxNodeIter::new(node);

        let keyword = it.expect_token_of_kind(SyntaxKind::KW_FUNCTION)?;

        let name = it.expect_token_of_kind(SyntaxKind::WORD)?;

        let param_list = it.expect_node_of_kind(SyntaxKind::PARAMETER_LIST)?;
        let params = FunctionParamList::from_cst(SyntaxElement::Node(param_list))?;

        let arrow = it.expect_token_of_kind(SyntaxKind::ARROW)?;

        let return_type = it.expect_node_of_kind(SyntaxKind::TYPE_EXPR)?;
        let return_type = Type::from_cst(SyntaxElement::Node(return_type))?;

        let body = it.expect_node("of kind LLM_FUNCTION_BODY or EXPR_FUNCTION_BODY")?;
        let body = FunctionDeclBody::from_cst(SyntaxElement::Node(body))?;

        it.expect_end()?;

        Ok(FunctionDecl {
            keyword: t::Function::new_from_span(keyword.text_range()),
            name: t::Word {
                token_span: name.text_range(),
            },
            params,
            arrow: t::Arrow::new_from_span(arrow.text_range()),
            return_type,
            body,
        })
    }
}

impl Printable for FunctionDecl {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print_raw_token(&self.name);
        printer.print(&self.params, shape.clone());
        printer.print_str(" ");
        printer.print_raw_token(&self.arrow);
        printer.print_str(" ");
        printer.print(&self.return_type, shape.clone());
        printer.print_str(" ");
        printer.print(&self.body, shape);
        PrintInfo::default_multi_lined()
    }
}

#[derive(Debug)]
pub struct FunctionParamList {
    pub open_paren: t::LParen,
    pub params: Vec<FunctionParam>,
    pub close_paren: t::RParen,
}
impl FromCST for FunctionParamList {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::PARAMETER_LIST)?;

        let mut visitor = SyntaxNodeIter::new(node);

        let open_paren = visitor.expect_token_of_kind(SyntaxKind::L_PAREN)?;

        let mut params = Vec::new();

        let close_paren = loop {
            let Some(elem) = visitor.next() else {
                return Err(StrongAstError::missing(
                    SyntaxKind::R_PAREN,
                    open_paren.text_range(),
                ));
            };
            match elem.kind() {
                SyntaxKind::PARAMETER => {
                    let param_node = StrongAstError::assert_is_node(elem)?;
                    params.push(FunctionParam::from_cst(SyntaxElement::Node(param_node))?);
                }
                SyntaxKind::R_PAREN => {
                    let token = StrongAstError::assert_is_token(elem)?;
                    break t::RParen::new_from_span(token.text_range());
                }
                _ => {
                    return Err(StrongAstError::UnexpectedAdditionalElement {
                        parent: open_paren.text_range(),
                        at: elem.text_range(),
                    });
                }
            }
        };

        visitor.expect_end()?;

        Ok(FunctionParamList {
            open_paren: t::LParen::new_from_span(open_paren.text_range()),
            params,
            close_paren,
        })
    }
}

impl Printable for FunctionParamList {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.open_paren);
        for (i, param) in self.params.iter().enumerate() {
            if i > 0 {
                printer.print_str(", ");
            }
            printer.print(param, shape.clone());
        }
        printer.print_raw_token(&self.close_paren);
        PrintInfo::default_single_line()
    }
}

#[derive(Debug)]
pub struct FunctionParam {
    pub name: t::Word,
    pub ty: Option<(Option<t::Colon>, Type)>,
}
impl FromCST for FunctionParam {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::PARAMETER)?;

        let mut visitor = SyntaxNodeIter::new(node);

        let name = visitor.expect_token_of_kind(SyntaxKind::WORD)?;

        let Some(colon) = visitor.next() else {
            // no type annotation. Valid in the case of `self`
            return Ok(FunctionParam {
                name: t::Word {
                    token_span: name.text_range(),
                },
                ty: None,
            });
        };

        if colon.kind() != SyntaxKind::COLON {
            return Err(StrongAstError::UnexpectedKind {
                expected: SyntaxKind::COLON,
                found: colon.kind(),
                at: colon.text_range(),
            });
        }

        let ty = visitor.expect_node_of_kind(SyntaxKind::TYPE_EXPR)?;
        let ty = Type::from_cst(SyntaxElement::Node(ty))?;

        visitor.expect_end()?;

        let ty = Some((Some(t::Colon::new_from_span(colon.text_range())), ty));
        Ok(FunctionParam {
            name: t::Word {
                token_span: name.text_range(),
            },
            ty,
        })
    }
}

impl Printable for FunctionParam {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.name);
        if let Some((colon, ty)) = &self.ty {
            if let Some(colon) = colon {
                printer.print_raw_token(colon);
            } else {
                printer.print_str(":");
            }
            printer.print_str(" ");
            printer.print(ty, shape);
        }
        PrintInfo::default_single_line()
    }
}

#[derive(Debug)]
pub enum FunctionDeclBody {
    Llm(LlmFunctionBody),
    Block(BlockExpr),
}
impl FromCST for FunctionDeclBody {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        match node.kind() {
            SyntaxKind::LLM_FUNCTION_BODY => Ok(FunctionDeclBody::Llm(LlmFunctionBody::from_cst(
                SyntaxElement::Node(node),
            )?)),
            SyntaxKind::EXPR_FUNCTION_BODY => {
                let mut visitor = SyntaxNodeIter::new(node);
                let block = visitor.expect_node_of_kind(SyntaxKind::BLOCK_EXPR)?;
                let block = BlockExpr::from_cst(SyntaxElement::Node(block))?;
                visitor.expect_end()?;
                Ok(FunctionDeclBody::Block(block))
            }
            _ => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "of kind LLM_FUNCTION_BODY or EXPR_FUNCTION_BODY".into(),
                found: node.kind(),
                at: node.text_range(),
            }),
        }
    }
}

impl Printable for FunctionDeclBody {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            FunctionDeclBody::Llm(llm) => llm.print(shape, printer),
            FunctionDeclBody::Block(block) => block.print(shape, printer),
        }
    }
}

#[derive(Debug)]
pub struct LlmFunctionBody {
    pub todo: TextRange, // TODO
}
impl FromCST for LlmFunctionBody {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::LLM_FUNCTION_BODY)?;

        return Ok(LlmFunctionBody {
            todo: node.text_range(),
        });
    }
}

impl Printable for LlmFunctionBody {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_input_range(self.todo);
        PrintInfo::default_multi_lined()
    }
}

#[derive(Debug)]
pub struct ClassDecl {
    pub keyword: t::Class,
    pub name: t::Word,
    pub open_brace: t::LBrace,
    pub items: Vec<ClassItem>,
    pub close_brace: t::RBrace,
}

impl FromCST for ClassDecl {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::CLASS_DEF)?;

        let mut it = SyntaxNodeIter::new(node);

        let keyword = it.expect_token_of_kind(SyntaxKind::KW_CLASS)?;

        let name = it.expect_token_of_kind(SyntaxKind::WORD)?;

        let open_brace = it.expect_token_of_kind(SyntaxKind::L_BRACE)?;

        // collect class items (fields, functions, block attributes)
        let mut items = Vec::new();

        let close_brace = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing(
                    SyntaxKind::R_BRACE,
                    open_brace.text_range(),
                ));
            };
            match elem.kind() {
                SyntaxKind::FIELD | SyntaxKind::FUNCTION_DEF | SyntaxKind::BLOCK_ATTRIBUTE => {
                    let item_node = StrongAstError::assert_is_node(elem)?;
                    items.push(ClassItem::from_cst(SyntaxElement::Node(item_node))?);
                }
                SyntaxKind::R_BRACE => {
                    let token = StrongAstError::assert_is_token(elem)?;
                    let close_brace = t::RBrace::new_from_span(token.text_range());
                    break close_brace;
                }
                _ => {
                    return Err(StrongAstError::UnexpectedAdditionalElement {
                        parent: open_brace.text_range(),
                        at: elem.text_range(),
                    });
                }
            }
        };

        it.expect_end()?;

        Ok(ClassDecl {
            keyword: t::Class::new_from_span(keyword.text_range()),
            name: t::Word {
                token_span: name.text_range(),
            },
            open_brace: t::LBrace::new_from_span(open_brace.text_range()),
            items,
            close_brace,
        })
    }
}

impl Printable for ClassDecl {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };

        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print_raw_token(&self.name);
        printer.print_str(" ");
        printer.print_raw_token(&self.open_brace);
        printer.print_newline();

        for item in &self.items {
            printer.print_spaces(inner_shape.indent);
            printer.print(item, inner_shape.clone());
            printer.print_newline();
        }

        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_brace);

        PrintInfo::default_multi_lined()
    }
}

#[derive(Debug)]
pub struct ClassField {
    pub name: t::Word,
    pub colon: Option<t::Colon>,
    pub ty: Type,
    pub attributes: Vec<Attribute>,
    pub comma: Option<t::Comma>,
}

impl FromCST for ClassField {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::FIELD)?;

        let mut it = SyntaxNodeIter::new(node);

        // name
        let name = it.expect_token_of_kind(SyntaxKind::WORD)?;

        // optional colon (fields can be defined without colons in BAML)
        let colon = it
            .next_if(|elem| elem.kind() == SyntaxKind::COLON)
            .map(|elem| {
                let colon = StrongAstError::assert_is_token(elem)?;
                Ok(t::Colon::new_from_span(colon.text_range()))
            })
            .transpose()?;

        // type expression
        let ty = it.expect_next("a type")?;
        let ty = Type::from_cst(ty)?;

        // collect attributes
        let mut attributes = Vec::new();
        let comma = loop {
            let Some(elem) = it.next() else {
                break None;
            };
            match elem.kind() {
                SyntaxKind::ATTRIBUTE => {
                    let attr_node = StrongAstError::assert_is_node(elem)?;
                    attributes.push(Attribute::from_cst(SyntaxElement::Node(attr_node))?);
                }
                SyntaxKind::COMMA => {
                    let comma = StrongAstError::assert_is_token(elem)?;
                    break Some(t::Comma::new_from_span(comma.text_range()));
                }
                found => {
                    return Err(StrongAstError::UnexpectedKindDesc {
                        expected_desc: "COMMA or ATTRIBUTE".into(),
                        found,
                        at: elem.text_range(),
                    });
                }
            }
        };

        Ok(ClassField {
            name: t::Word::new_from_span(name.text_range()),
            colon,
            ty,
            attributes,
            comma,
        })
    }
}

impl Printable for ClassField {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.name);
        printer.print_str(" ");
        printer.print(&self.ty, shape.clone());

        for attr in &self.attributes {
            printer.print_str(" ");
            printer.print(attr, shape.clone());
        }

        PrintInfo::default_single_line()
    }
}

#[derive(Debug)]
pub enum ClassItem {
    Field(ClassField),
    Function(FunctionDecl),
    BlockAttribute(BlockAttribute),
}

impl FromCST for ClassItem {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        let item = match node.kind() {
            SyntaxKind::FIELD => ClassItem::Field(ClassField::from_cst(SyntaxElement::Node(node))?),
            SyntaxKind::FUNCTION_DEF => {
                ClassItem::Function(FunctionDecl::from_cst(SyntaxElement::Node(node))?)
            }
            SyntaxKind::BLOCK_ATTRIBUTE => {
                ClassItem::BlockAttribute(BlockAttribute::from_cst(SyntaxElement::Node(node))?)
            }
            found => {
                return Err(StrongAstError::UnexpectedKindDesc {
                    expected_desc: "FIELD, FUNCTION_DEF, or BLOCK_ATTRIBUTE".into(),
                    found,
                    at: node.text_range(),
                });
            }
        };
        Ok(item)
    }
}

impl Printable for ClassItem {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            ClassItem::Field(field) => field.print(shape, printer),
            ClassItem::Function(function) => function.print(shape, printer),
            ClassItem::BlockAttribute(attr) => attr.print(shape, printer),
        }
    }
}

#[derive(Debug)]
pub struct EnumDecl {
    pub keyword: t::Enum,
    pub name: t::Word,
    pub open_brace: t::LBrace,
    pub items: Vec<EnumItem>,
    pub close_brace: t::RBrace,
}

impl FromCST for EnumDecl {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::ENUM_DEF)?;

        let enum_range = node.text_range();
        let mut it = SyntaxNodeIter::new(node);

        // keyword: "enum"
        let keyword = it.expect_token_of_kind(SyntaxKind::KW_ENUM)?;

        // name
        let name = it.expect_token_of_kind(SyntaxKind::WORD)?;

        // open brace
        let open_brace = it.expect_token_of_kind(SyntaxKind::L_BRACE)?;

        let mut items = Vec::new();
        let close_brace = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing_desc(
                    "kinds ENUM_VARIANT, BLOCK_ATTRIBUTE, or R_BRACE",
                    enum_range,
                ));
            };
            match elem.kind() {
                SyntaxKind::ENUM_VARIANT => {
                    let variant = StrongAstError::assert_is_node(elem)?;
                    let variant = EnumVariant::from_cst(SyntaxElement::Node(variant))?;

                    let comma = it
                        .next_if(|elem| elem.kind() == SyntaxKind::COMMA)
                        .map(|comma| {
                            let comma = StrongAstError::assert_is_token(comma)?;
                            Ok(t::Comma::new_from_span(comma.text_range()))
                        })
                        .transpose()?;

                    items.push(EnumItem::Variant(variant, comma));
                }
                SyntaxKind::BLOCK_ATTRIBUTE => {
                    let attr_node = StrongAstError::assert_is_node(elem)?;
                    let attr = BlockAttribute::from_cst(SyntaxElement::Node(attr_node))?;
                    items.push(EnumItem::BlockAttribute(attr));
                }
                SyntaxKind::R_BRACE => {
                    let close_brace = StrongAstError::assert_is_token(elem)?;
                    let close_brace = t::RBrace::new_from_span(close_brace.text_range());
                    break close_brace;
                }
                _ => {
                    return Err(StrongAstError::UnexpectedKindDesc {
                        expected_desc: "kinds ENUM_VARIANT, BLOCK_ATTRIBUTE, or R_BRACE".into(),
                        found: elem.kind(),
                        at: elem.text_range(),
                    });
                }
            }
        };

        it.expect_end()?;

        Ok(EnumDecl {
            keyword: t::Enum::new_from_span(keyword.text_range()),
            name: t::Word {
                token_span: name.text_range(),
            },
            open_brace: t::LBrace::new_from_span(open_brace.text_range()),
            items,
            close_brace,
        })
    }
}

impl Printable for EnumDecl {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };

        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print_raw_token(&self.name);
        printer.print_str(" ");
        printer.print_raw_token(&self.open_brace);
        printer.print_newline();

        for item in &self.items {
            printer.print_spaces(inner_shape.indent);
            printer.print(item, inner_shape.clone());
            printer.print_newline();
        }

        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_brace);

        PrintInfo::default_multi_lined()
    }
}

#[derive(Debug)]
pub enum EnumItem {
    Variant(EnumVariant, Option<t::Comma>),
    BlockAttribute(BlockAttribute),
}

impl Printable for EnumItem {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            EnumItem::Variant(variant, _comma) => variant.print(shape, printer),
            EnumItem::BlockAttribute(attr) => attr.print(shape, printer),
        }
    }
}

#[derive(Debug)]
pub struct EnumVariant {
    pub name: t::Word,
    pub attributes: Vec<Attribute>,
}

impl FromCST for EnumVariant {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::ENUM_VARIANT)?;

        let mut it = SyntaxNodeIter::new(node);

        let name = it.expect_token_of_kind(SyntaxKind::WORD)?;

        let attributes = it.map(Attribute::from_cst).collect::<Result<_, _>>()?;

        Ok(EnumVariant {
            name: t::Word::new_from_span(name.text_range()),
            attributes,
        })
    }
}

impl Printable for EnumVariant {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.name);

        for attr in &self.attributes {
            printer.print_str(" ");
            printer.print(attr, shape.clone());
        }

        PrintInfo::default_single_line()
    }
}

#[derive(Debug)]
pub struct ClientDecl {
    pub keyword: t::Client,
    pub rangle: t::Less,
    pub generic: t::Word,
    pub langle: t::Greater,
    pub name: t::Word,
    pub config_block: ConfigBlock,
}

impl FromCST for ClientDecl {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::CLIENT_DEF)?;

        let mut it = SyntaxNodeIter::new(node);

        // keyword: "client"
        let keyword = it.expect_token_of_kind(SyntaxKind::KW_CLIENT)?;

        // client type: <llm>
        let client_type_node = it.expect_node_of_kind(SyntaxKind::CLIENT_TYPE)?;

        // Parse client type to get <, generic, >
        let mut ct_visitor = SyntaxNodeIter::new(client_type_node.clone());

        let rangle = ct_visitor.expect_token_of_kind(SyntaxKind::LESS)?;
        let generic = ct_visitor.expect_token_of_kind(SyntaxKind::WORD)?;
        let langle = ct_visitor.expect_token_of_kind(SyntaxKind::GREATER)?;
        ct_visitor.expect_end()?;

        // name
        let name = it.expect_token_of_kind(SyntaxKind::WORD)?;

        // config block
        let config_block = it.expect_node_of_kind(SyntaxKind::CONFIG_BLOCK)?;
        let config_block = ConfigBlock::from_cst(SyntaxElement::Node(config_block))?;

        it.expect_end()?;

        Ok(ClientDecl {
            keyword: t::Client::new_from_span(keyword.text_range()),
            rangle: t::Less::new_from_span(rangle.text_range()),
            generic: t::Word {
                token_span: generic.text_range(),
            },
            langle: t::Greater::new_from_span(langle.text_range()),
            name: t::Word {
                token_span: name.text_range(),
            },
            config_block,
        })
    }
}

impl Printable for ClientDecl {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_raw_token(&self.rangle);
        printer.print_raw_token(&self.generic);
        printer.print_raw_token(&self.langle);
        printer.print_str(" ");
        printer.print_raw_token(&self.name);
        printer.print_str(" ");
        printer.print(&self.config_block, shape);
        PrintInfo::default_multi_lined()
    }
}

/// Corresponds to a [`SyntaxKind::CONFIG_BLOCK`] node.
#[derive(Debug)]
pub struct ConfigBlock {
    pub open_brace: t::LBrace,
    pub items: Vec<(ConfigItem, Option<t::Comma>)>,
    pub close_brace: t::RBrace,
}

impl FromCST for ConfigBlock {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::CONFIG_BLOCK)?;

        let mut it = SyntaxNodeIter::new(node);

        let open_brace = it.expect_token_of_kind(SyntaxKind::L_BRACE)?;

        let mut items = Vec::new();
        let close_brace = loop {
            let elem = it.expect_next("CONFIG_ITEM or R_BRACE")?;
            if elem.kind() == SyntaxKind::R_BRACE {
                let token = StrongAstError::assert_is_token(elem)?;
                break t::RBrace::new_from_span(token.text_range());
            }

            let item = ConfigItem::from_cst(elem)?;
            let comma = it
                .next_if(|elem| elem.kind() == SyntaxKind::COMMA)
                .map(|comma| {
                    let comma = StrongAstError::assert_is_token(comma)?;
                    Ok(t::Comma::new_from_span(comma.text_range()))
                })
                .transpose()?;

            items.push((item, comma));
        };
        Ok(ConfigBlock {
            open_brace: t::LBrace::new_from_span(open_brace.text_range()),
            items,
            close_brace,
        })
    }
}

impl Printable for ConfigBlock {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };

        printer.print_raw_token(&self.open_brace);
        printer.print_newline();

        for (item, _comma) in &self.items {
            printer.print_spaces(inner_shape.indent);
            printer.print(item, inner_shape.clone());
            printer.print_newline();
        }

        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_brace);

        PrintInfo::default_multi_lined()
    }
}

/// Corresponds to a [`SyntaxKind::CONFIG_ITEM`] node.
#[derive(Debug)]
pub struct ConfigItem {
    pub key: t::Word,
    // /// Colons are currently invalid, it seems
    // pub colon: Option<t::Colon>,
    pub value: ConfigItemValue,
}

impl FromCST for ConfigItem {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::CONFIG_ITEM)?;

        let mut it = SyntaxNodeIter::new(node);

        let key = it.expect_token_of_kind(SyntaxKind::WORD)?;

        // let colon = it
        //     .next_if(|elem| elem.kind() == SyntaxKind::COLON)
        //     .map(|elem| {
        //         let colon = StrongAstError::assert_is_token(elem)?;
        //         Ok(t::Colon::new_from_span(colon.text_range()))
        //     })
        //     .transpose()?;

        let value = it.expect_next("a config value")?;
        let value = ConfigItemValue::from_cst(value)?;

        it.expect_end()?;

        Ok(ConfigItem {
            key: t::Word::new_from_span(key.text_range()),
            // colon,
            value,
        })
    }
}

impl Printable for ConfigItem {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.key);
        printer.print_str(" ");
        printer.print(&self.value, shape);
        PrintInfo::default_single_line()
    }
}

/// Does not correspond to a specific [`SyntaxKind`].
#[derive(Debug)]
pub enum ConfigItemValue {
    Value(Expression),
    NestedBlock(ConfigBlock),
}

impl FromCST for ConfigItemValue {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        match node.kind() {
            SyntaxKind::CONFIG_VALUE => {
                let value = Expression::from_cst(SyntaxElement::Node(node))?;
                Ok(ConfigItemValue::Value(value))
            }
            SyntaxKind::CONFIG_BLOCK => {
                let block = ConfigBlock::from_cst(SyntaxElement::Node(node))?;
                Ok(ConfigItemValue::NestedBlock(block))
            }
            _ => Err(StrongAstError::UnexpectedKind {
                expected: SyntaxKind::CONFIG_VALUE,
                found: node.kind(),
                at: node.text_range(),
            }),
        }
    }
}

impl Printable for ConfigItemValue {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            ConfigItemValue::Value(expr) => expr.print(shape, printer),
            ConfigItemValue::NestedBlock(block) => block.print(shape, printer),
        }
    }
}

#[derive(Debug)]
pub struct TestDecl {
    pub keyword: t::Test,
    pub name: t::Word,
    pub open_brace: t::LBrace,
    pub functions: TextRange, // TODO
    pub close_brace: t::RBrace,
}

impl FromCST for TestDecl {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::TEST_DEF)?;

        let mut it = SyntaxNodeIter::new(node);

        // keyword: "test"
        let keyword = it.expect_token_of_kind(SyntaxKind::KW_TEST)?;

        // name
        let name = it.expect_token_of_kind(SyntaxKind::WORD)?;

        // config block
        let config_block_node = it.expect_node_of_kind(SyntaxKind::CONFIG_BLOCK)?;

        // Parse config block to get braces
        let mut cb_visitor = SyntaxNodeIter::new(config_block_node.clone());

        let open_brace = cb_visitor.expect_token_of_kind(SyntaxKind::L_BRACE)?;

        // Find the closing brace by scanning to the end
        let mut close_brace_token = None;
        while let Some(elem) = cb_visitor.next() {
            if elem.kind() == SyntaxKind::R_BRACE {
                close_brace_token = Some(StrongAstError::assert_is_token(elem)?);
            }
        }

        let close_brace = close_brace_token.ok_or_else(|| {
            StrongAstError::missing(SyntaxKind::R_BRACE, config_block_node.text_range())
        })?;

        it.expect_end()?;

        Ok(TestDecl {
            keyword: t::Test::new_from_span(keyword.text_range()),
            name: t::Word {
                token_span: name.text_range(),
            },
            open_brace: t::LBrace::new_from_span(open_brace.text_range()),
            functions: config_block_node.text_range(), // TODO: Parse the actual functions
            close_brace: t::RBrace::new_from_span(close_brace.text_range()),
        })
    }
}

impl Printable for TestDecl {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print_raw_token(&self.name);
        printer.print_str(" ");
        printer.print_input_range(self.functions);
        PrintInfo::default_multi_lined()
    }
}

#[derive(Debug)]
pub struct RetryPolicyDecl {
    pub keyword: t::RetryPolicy,
    pub name: t::Word,
    pub open_brace: t::LBrace,
    pub config_block: TextRange, // TODO
    pub close_brace: t::RBrace,
}

impl FromCST for RetryPolicyDecl {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::RETRY_POLICY_DEF)?;

        let mut it = SyntaxNodeIter::new(node);

        // keyword: "retry_policy"
        let keyword = it.expect_token_of_kind(SyntaxKind::KW_RETRY_POLICY)?;

        // name
        let name = it.expect_token_of_kind(SyntaxKind::WORD)?;

        // config block
        let config_block_node = it.expect_node_of_kind(SyntaxKind::CONFIG_BLOCK)?;

        // Parse config block to get braces
        let mut cb_visitor = SyntaxNodeIter::new(config_block_node.clone());

        let open_brace = cb_visitor.expect_token_of_kind(SyntaxKind::L_BRACE)?;

        // Find the closing brace by scanning to the end
        let mut close_brace_token = None;
        while let Some(elem) = cb_visitor.next() {
            if elem.kind() == SyntaxKind::R_BRACE {
                close_brace_token = Some(StrongAstError::assert_is_token(elem)?);
            }
        }

        let close_brace = close_brace_token.ok_or_else(|| {
            StrongAstError::missing(SyntaxKind::R_BRACE, config_block_node.text_range())
        })?;

        it.expect_end()?;

        Ok(RetryPolicyDecl {
            keyword: t::RetryPolicy::new_from_span(keyword.text_range()),
            name: t::Word {
                token_span: name.text_range(),
            },
            open_brace: t::LBrace::new_from_span(open_brace.text_range()),
            config_block: config_block_node.text_range(), // TODO: Parse the actual config
            close_brace: t::RBrace::new_from_span(close_brace.text_range()),
        })
    }
}

// pub struct TemplateStringDecl {
//     pub keyword: t::TemplateString,
//     pub name: t::Word,
//     pub open_brace: t::LBrace,
//     pub template_string: (),
//     pub close_brace: t::RBrace,
// }
// impl Declaration for TemplateStringDecl {}

#[derive(Debug)]
pub struct TypeAliasDecl {
    /// For some reason, type is not currently a keyword
    pub keyword: t::Word,
    pub name: t::Word,
    pub equals: t::Equals,
    pub type_expr: Type,
    pub semicolon: Option<t::Semicolon>,
}

impl FromCST for TypeAliasDecl {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::TYPE_ALIAS_DEF)?;

        let mut it = SyntaxNodeIter::new(node);

        // keyword: "type" (it's actually just a WORD, not a keyword)
        let keyword = it.expect_token_of_kind(SyntaxKind::WORD)?;

        // name
        let name = it.expect_token_of_kind(SyntaxKind::WORD)?;

        // equals
        let equals = it.expect_token_of_kind(SyntaxKind::EQUALS)?;

        // type expression
        let type_expr = it.expect_node_of_kind(SyntaxKind::TYPE_EXPR)?;
        let type_expr = Type::from_cst(SyntaxElement::Node(type_expr))?;

        // optional semicolon
        let semicolon = it.next().and_then(|elem| {
            if elem.kind() == SyntaxKind::SEMICOLON {
                elem.as_token()
                    .map(|t| t::Semicolon::new_from_span(t.text_range()))
            } else {
                None
            }
        });

        Ok(TypeAliasDecl {
            keyword: t::Word {
                token_span: keyword.text_range(),
            },
            name: t::Word {
                token_span: name.text_range(),
            },
            equals: t::Equals::new_from_span(equals.text_range()),
            type_expr,
            semicolon,
        })
    }
}

impl Printable for TypeAliasDecl {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print_raw_token(&self.name);
        printer.print_str(" ");
        printer.print_raw_token(&self.equals);
        printer.print_str(" ");
        printer.print(&self.type_expr, shape);

        if let Some(semicolon) = &self.semicolon {
            printer.print_raw_token(semicolon);
        }

        PrintInfo::default_single_line()
    }
}
