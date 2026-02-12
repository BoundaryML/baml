use baml_compiler_syntax::{SyntaxElement, SyntaxKind};
use rowan::TextRange;

use crate::ast::{
    Attribute, BlockAttribute, BlockExpr, Expression, FromCST, KnownKind, PathExpr, StrongAstError,
    SyntaxNodeIter, Token, Type, tokens as t,
};
use crate::printer::*;

#[derive(Debug)]
pub enum TopLevelDeclaration {
    Function(FunctionDecl),
    Class(ClassDecl),
    Enum(EnumDecl),
    Client(ClientDecl),
    Test(TestDecl),
    RetryPolicy(RetryPolicyDecl),
    TemplateString(TemplateStringDecl),
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
                TopLevelDeclaration::RetryPolicy(RetryPolicyDecl::from_cst(elem)?)
            }
            SyntaxKind::TEMPLATE_STRING_DEF => {
                TopLevelDeclaration::TemplateString(TemplateStringDecl::from_cst(elem)?)
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
            TopLevelDeclaration::RetryPolicy(retry_policy_decl) => {
                retry_policy_decl.print(shape, printer)
            }
            TopLevelDeclaration::TemplateString(template_string) => {
                template_string.print(shape, printer)
            }
            TopLevelDeclaration::TypeAlias(type_alias_decl) => {
                type_alias_decl.print(shape, printer)
            }
            TopLevelDeclaration::Unknown(range) => {
                // May not be idempotent due to whitespace changes, but that's okay because we shouldn't
                // have unknown stuff anyway.
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

        let keyword = it.expect_parse()?;

        let name = it.expect_parse()?;

        let params: FunctionParamList = it.expect_parse()?;

        let arrow = it.expect_parse()?;

        let return_type: Type = it.expect_parse()?;

        let body = it.expect_node("of kind LLM_FUNCTION_BODY or EXPR_FUNCTION_BODY")?;
        let body = FunctionDeclBody::from_cst(SyntaxElement::Node(body))?;

        it.expect_end()?;

        Ok(FunctionDecl {
            keyword,
            name,
            params,
            arrow,
            return_type,
            body,
        })
    }
}

impl KnownKind for FunctionDecl {
    fn kind() -> SyntaxKind {
        SyntaxKind::FUNCTION_DEF
    }
}

impl Printable for FunctionDecl {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print_raw_token(&self.name);

        let mut param_printer = Printer::new_empty(printer.input, printer.config, printer.trivia);
        let param_info = param_printer.print(&self.params, Shape::unlimited_single_line());

        let mut return_type_printer =
            Printer::new_empty(printer.input, printer.config, printer.trivia);
        let return_type_info =
            return_type_printer.print(&self.return_type, Shape::unlimited_single_line());

        let single_line_size = printer.current_line_len()
            + param_printer.output.len()
            + const { " -> ".len() + " {".len() }
            + return_type_printer.output.len();
        if single_line_size <= printer.config.line_width
            && !param_info.multi_lined
            && !return_type_info.multi_lined
        {
            // It fits in single line!
            printer.append_from_printer(param_printer);
            printer.print_spaces(1);
            printer.print_raw_token(&self.arrow);
            printer.print_spaces(1);
            printer.append_from_printer(return_type_printer);
            printer.print_spaces(1);
            printer.print(&self.body, shape)
        } else {
            let params_shape = Shape {
                width: 0, // never single-line
                indent: shape.indent,
                first_line_offset: 0, // not important in function args
            };
            let _ = self.params.print_multi_line(params_shape, printer);

            printer.print_spaces(1);
            printer.print_raw_token(&self.arrow);
            printer.print_spaces(1);

            let curr_line_len = printer.current_line_len();
            let return_type_shape = Shape {
                width: printer
                    .config
                    .line_width
                    .saturating_sub(curr_line_len + const { " {".len() }),
                indent: shape.indent,
                first_line_offset: curr_line_len.saturating_sub(shape.indent),
            };

            let return_info = self.return_type.print(return_type_shape, printer);
            if return_info.multi_lined && self.return_type.multi_line_is_indented() {
                // `{` goes on its own line after the type ends
                printer.print_newline();
            } else {
                printer.print_str(" ");
            }

            printer.print(&self.body, shape);

            PrintInfo::default_multi_lined()
        }
    }
}

#[derive(Debug)]
pub struct FunctionParamList {
    pub open_paren: t::LParen,
    pub params: Vec<(FunctionParam, Option<t::Comma>)>,
    pub close_paren: t::RParen,
}
impl FromCST for FunctionParamList {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::PARAMETER_LIST)?;

        let mut it = SyntaxNodeIter::new(node);

        let open_paren = it.expect_parse()?;

        let mut params = Vec::new();

        let close_paren = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing(SyntaxKind::R_PAREN, it.parent));
            };
            match elem.kind() {
                SyntaxKind::PARAMETER => {
                    let param = FunctionParam::from_cst(elem)?;
                    let comma = it
                        .next_if_kind(SyntaxKind::COMMA)
                        .map(t::Comma::from_cst)
                        .transpose()?;
                    params.push((param, comma));
                }
                SyntaxKind::R_PAREN => {
                    break t::RParen::from_cst(elem)?;
                }
                _ => {
                    return Err(StrongAstError::UnexpectedAdditionalElement {
                        parent: it.parent,
                        at: elem.text_range(),
                    });
                }
            }
        };

        it.expect_end()?;

        Ok(FunctionParamList {
            open_paren,
            params,
            close_paren,
        })
    }
}

impl KnownKind for FunctionParamList {
    fn kind() -> SyntaxKind {
        SyntaxKind::PARAMETER_LIST
    }
}

impl PrintMultiLine for FunctionParamList {
    /// Multi-line layout: each parameter on its own indented line with trailing comma.
    /// Closing paren on its own line.
    ///
    /// ```baml
    /// (
    ///     first: string,
    ///     second: int,
    ///     third: bool,
    /// )
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };

        printer.print_raw_token(&self.open_paren);
        printer.print_newline();

        for (param, comma) in &self.params {
            printer.print_spaces(inner_shape.indent);
            printer.print(param, inner_shape.clone());
            if let Some(comma) = comma {
                printer.print_raw_token(comma);
            } else {
                printer.print_str(",");
            }
            printer.print_newline();
        }

        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_paren);
        PrintInfo::default_multi_lined()
    }
}

impl Printable for FunctionParamList {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        let mut single_line_printer =
            Printer::new_empty(printer.input, printer.config, printer.trivia);
        single_line_printer.print_raw_token(&self.open_paren);
        for (i, (param, comma)) in self.params.iter().enumerate() {
            multi_lined |= single_line_printer
                .print(param, Shape::unlimited_single_line())
                .multi_lined;
            if i + 1 < self.params.len() {
                if let Some(comma) = comma {
                    single_line_printer.print_raw_token(comma);
                } else {
                    single_line_printer.print_str(",");
                }
                single_line_printer.print_str(" ");
            }
            if multi_lined || single_line_printer.output.len() > shape.width {
                return Self::print_multi_line(self, shape, printer);
            }
        }
        single_line_printer.print_raw_token(&self.close_paren);

        if multi_lined || single_line_printer.output.len() > shape.width {
            Self::print_multi_line(self, shape, printer)
        } else {
            printer.append_from_printer(single_line_printer);
            PrintInfo::default_single_line()
        }
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

        let mut it = SyntaxNodeIter::new(node);

        let name = it.expect_parse()?;

        let colon = it
            .next_if_kind(SyntaxKind::COLON)
            .map(t::Colon::from_cst)
            .transpose()?;

        let ty = if let Some(colon) = colon {
            // If there is a colon, there MUST be a type
            Some((Some(colon), it.expect_parse()?))
        } else {
            // If there is no colon, type is optional (e.g. `self` lacks a type)
            it.next_if_kind(SyntaxKind::TYPE_EXPR)
                .map(Type::from_cst)
                .transpose()?
                .map(|ty| (None, ty))
        };

        it.expect_end()?;

        Ok(FunctionParam { name, ty })
    }
}

impl KnownKind for FunctionParam {
    fn kind() -> SyntaxKind {
        SyntaxKind::PARAMETER
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
                let block: BlockExpr = visitor.expect_parse()?;
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

/// Corresponds to a [`SyntaxKind::LLM_FUNCTION_BODY`] node.
#[derive(Debug)]
pub struct LlmFunctionBody {
    pub open_brace: t::LBrace,
    /// Not guaranteed that client is before prompt in the input.
    pub client: ClientField,
    /// Not guaranteed that client is before prompt in the input.
    pub prompt: PromptField,
    pub close_brace: t::RBrace,
}
impl FromCST for LlmFunctionBody {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::LLM_FUNCTION_BODY)?;

        let mut it = SyntaxNodeIter::new(node);

        let open_brace = it.expect_parse()?;

        let first = it.expect_node("CLIENT_FIELD or PROMPT_FIELD")?;
        let (client, prompt) = match first.kind() {
            SyntaxKind::CLIENT_FIELD => {
                let client = ClientField::from_cst(SyntaxElement::Node(first))?;
                let prompt: PromptField = it.expect_parse()?;
                (client, prompt)
            }
            SyntaxKind::PROMPT_FIELD => {
                let prompt = PromptField::from_cst(SyntaxElement::Node(first))?;
                let client: ClientField = it.expect_parse()?;
                (client, prompt)
            }
            found => {
                return Err(StrongAstError::UnexpectedKindDesc {
                    expected_desc: "CLIENT_FIELD or PROMPT_FIELD".into(),
                    found,
                    at: first.text_range(),
                });
            }
        };

        let close_brace = it.expect_parse()?;

        it.expect_end()?;

        return Ok(LlmFunctionBody {
            open_brace,
            client,
            prompt,
            close_brace,
        });
    }
}

impl KnownKind for LlmFunctionBody {
    fn kind() -> SyntaxKind {
        SyntaxKind::LLM_FUNCTION_BODY
    }
}

impl Printable for LlmFunctionBody {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.open_brace);
        printer.print_newline();

        let inner_indent = shape.indent + printer.config.indent_width;
        let inner_shape = Shape {
            width: printer.config.line_width.saturating_sub(inner_indent),
            indent: inner_indent,
            first_line_offset: 0,
        };
        printer.print_spaces(inner_indent);
        printer.print(&self.client, inner_shape.clone());
        printer.print_newline();
        printer.print_spaces(inner_shape.indent);
        printer.print(&self.prompt, inner_shape);

        printer.print_newline();
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_brace);
        PrintInfo::default_multi_lined()
    }
}

/// Corresponds to a [`SyntaxKind::CLIENT_FIELD`] node.
#[derive(Debug)]
pub struct ClientField {
    pub keyword: t::Client,
    // not currently allowed
    // pub colon: Option<t::Colon>,
    pub name: ClientName,
}

impl FromCST for ClientField {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::CLIENT_FIELD)?;

        let mut it = SyntaxNodeIter::new(node);

        let keyword = it.expect_parse()?;

        let name = it.expect_next("STRING_LITERAL, WORD, or PATH_EXPR")?;
        let name = match name.kind() {
            SyntaxKind::STRING_LITERAL => ClientName::String(t::QuotedString::from_cst(name)?),
            SyntaxKind::WORD => {
                // Not actually a PATH_EXPR, but we'll treat it as one since the CST currently doesn't handle this.
                let first = t::Word::from_cst(name)?;
                let mut rest = Vec::new();
                while let Some(dot) = it.next_if_kind(SyntaxKind::DOT) {
                    let dot = t::Dot::from_cst(dot)?;
                    let word = it.expect_parse()?;
                    rest.push((dot, word));
                }
                ClientName::Path(PathExpr { first, rest })
            }
            SyntaxKind::PATH_EXPR => ClientName::Path(PathExpr::from_cst(name)?),
            found => {
                return Err(StrongAstError::UnexpectedKindDesc {
                    expected_desc: "STRING_LITERAL, WORD, or PATH_EXPR".into(),
                    found: found,
                    at: name.text_range(),
                });
            }
        };

        it.expect_end()?;

        Ok(ClientField { keyword, name })
    }
}

impl KnownKind for ClientField {
    fn kind() -> SyntaxKind {
        SyntaxKind::CLIENT_FIELD
    }
}

impl Printable for ClientField {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print(&self.name, shape)
    }
}

#[derive(Debug)]
pub enum ClientName {
    Path(PathExpr),
    String(t::QuotedString),
}

impl Printable for ClientName {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            ClientName::Path(path) => printer.print(path, shape),
            ClientName::String(string) => printer.print(string, shape),
        }
    }
}

#[derive(Debug)]
pub struct PromptField {
    pub prompt: t::Word,
    pub raw_string: t::RawString,
}

impl FromCST for PromptField {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::PROMPT_FIELD)?;

        let mut it = SyntaxNodeIter::new(node);

        // It's a word, but we should never be in a `PROMPT_FIELD` context if it's not a prompt
        let prompt = it.expect_parse()?;

        let raw_string: t::RawString = it.expect_parse()?;

        it.expect_end()?;

        Ok(PromptField { prompt, raw_string })
    }
}

impl KnownKind for PromptField {
    fn kind() -> SyntaxKind {
        SyntaxKind::PROMPT_FIELD
    }
}

impl Printable for PromptField {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.prompt);
        printer.print_str(" ");
        printer.print(&self.raw_string, shape)
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

        let keyword = it.expect_parse()?;

        let name = it.expect_parse()?;

        let open_brace = it.expect_parse()?;

        // collect class items (fields, functions, block attributes)
        let mut items = Vec::new();

        let close_brace = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing(SyntaxKind::R_BRACE, it.parent));
            };
            match elem.kind() {
                SyntaxKind::FIELD | SyntaxKind::FUNCTION_DEF | SyntaxKind::BLOCK_ATTRIBUTE => {
                    items.push(ClassItem::from_cst(elem)?);
                }
                SyntaxKind::R_BRACE => {
                    break t::RBrace::from_cst(elem)?;
                }
                _ => {
                    return Err(StrongAstError::UnexpectedAdditionalElement {
                        parent: it.parent,
                        at: elem.text_range(),
                    });
                }
            }
        };

        it.expect_end()?;

        Ok(ClassDecl {
            keyword,
            name,
            open_brace,
            items,
            close_brace,
        })
    }
}

impl KnownKind for ClassDecl {
    fn kind() -> SyntaxKind {
        SyntaxKind::CLASS_DEF
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

        let name = it.expect_parse()?;

        // optional colon (fields can be defined without colons in BAML)
        let colon = it
            .next_if_kind(SyntaxKind::COLON)
            .map(t::Colon::from_cst)
            .transpose()?;

        // type expression
        let ty: Type = it.expect_parse()?;

        // collect attributes
        let mut attributes = Vec::new();
        let comma = loop {
            let Some(elem) = it.next() else {
                break None;
            };
            match elem.kind() {
                SyntaxKind::ATTRIBUTE => {
                    attributes.push(Attribute::from_cst(elem)?);
                }
                SyntaxKind::COMMA => {
                    break Some(t::Comma::from_cst(elem)?);
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
            name,
            colon,
            ty,
            attributes,
            comma,
        })
    }
}

impl KnownKind for ClassField {
    fn kind() -> SyntaxKind {
        SyntaxKind::FIELD
    }
}

impl PrintMultiLine for ClassField {
    /// Multi-line layout: attributes wrap to their own indented lines
    /// below the field name and type. Per spec, attributes are moved to
    /// own lines before the type itself is multi-lined.
    ///
    /// ```baml
    /// myField ReallyLongTypeName
    ///     @alias("theLongField")
    ///     @description("some desc")
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let attr_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };

        printer.print_raw_token(&self.name);
        printer.print_str(" ");
        printer.print(&self.ty, shape.clone());
        for attr in &self.attributes {
            printer.print_newline();
            printer.print_spaces(attr_shape.indent);
            printer.print(attr, attr_shape.clone());
        }

        PrintInfo::default_multi_lined()
    }
}

impl Printable for ClassField {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut single_line_printer =
            Printer::new_empty(printer.input, printer.config, printer.trivia);
        single_line_printer.print_raw_token(&self.name);
        single_line_printer.print_str(" ");
        let mut multi_lined = single_line_printer
            .print(&self.ty, Shape::unlimited_single_line())
            .multi_lined;
        for attr in &self.attributes {
            single_line_printer.print_str(" ");
            multi_lined |= single_line_printer
                .print(attr, Shape::unlimited_single_line())
                .multi_lined;
        }

        if multi_lined || single_line_printer.output.len() > shape.width {
            Self::print_multi_line(self, shape, printer)
        } else {
            printer.append_from_printer(single_line_printer);
            PrintInfo::default_single_line()
        }
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
        let item = match elem.kind() {
            SyntaxKind::FIELD => ClassItem::Field(ClassField::from_cst(elem)?),
            SyntaxKind::FUNCTION_DEF => ClassItem::Function(FunctionDecl::from_cst(elem)?),
            SyntaxKind::BLOCK_ATTRIBUTE => {
                ClassItem::BlockAttribute(BlockAttribute::from_cst(elem)?)
            }
            found => {
                return Err(StrongAstError::UnexpectedKindDesc {
                    expected_desc: "FIELD, FUNCTION_DEF, or BLOCK_ATTRIBUTE".into(),
                    found,
                    at: elem.text_range(),
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
        let keyword = it.expect_parse()?;

        // name
        let name = it.expect_parse()?;

        // open brace
        let open_brace = it.expect_parse()?;

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
                        .next_if_kind(SyntaxKind::COMMA)
                        .map(t::Comma::from_cst)
                        .transpose()?;

                    items.push(EnumItem::Variant(variant, comma));
                }
                SyntaxKind::BLOCK_ATTRIBUTE => {
                    let attr = BlockAttribute::from_cst(elem)?;
                    items.push(EnumItem::BlockAttribute(attr));
                }
                SyntaxKind::R_BRACE => {
                    break t::RBrace::from_cst(elem)?;
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
            keyword,
            name,
            open_brace,
            items,
            close_brace,
        })
    }
}

impl KnownKind for EnumDecl {
    fn kind() -> SyntaxKind {
        SyntaxKind::ENUM_DEF
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
            EnumItem::Variant(variant, comma) => {
                let info = variant.print(shape, printer);
                if let Some(comma) = &comma {
                    printer.print_raw_token(comma);
                } else {
                    printer.print_str(",");
                }
                info
            }
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

        let name = it.expect_parse()?;

        let attributes = it.map(Attribute::from_cst).collect::<Result<_, _>>()?;

        Ok(EnumVariant { name, attributes })
    }
}

impl KnownKind for EnumVariant {
    fn kind() -> SyntaxKind {
        SyntaxKind::ENUM_VARIANT
    }
}

impl PrintMultiLine for EnumVariant {
    /// Multi-line layout: attributes wrap to their own indented lines
    /// below the variant name. Same attribute rules as [`ClassField`].
    ///
    /// ```baml
    /// VariantName
    ///     @alias("something_long")
    ///     @description("a long description")
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let attr_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };

        printer.print_raw_token(&self.name);
        for attr in &self.attributes {
            printer.print_newline();
            printer.print_spaces(attr_shape.indent);
            printer.print(attr, attr_shape.clone());
        }

        PrintInfo::default_multi_lined()
    }
}

impl Printable for EnumVariant {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut single_line_printer =
            Printer::new_empty(printer.input, printer.config, printer.trivia);
        single_line_printer.print_raw_token(&self.name);
        let mut multi_lined = false;
        for attr in &self.attributes {
            single_line_printer.print_str(" ");
            multi_lined |= single_line_printer
                .print(attr, Shape::unlimited_single_line())
                .multi_lined;
        }

        if multi_lined || single_line_printer.output.len() > shape.width {
            Self::print_multi_line(self, shape, printer)
        } else {
            printer.append_from_printer(single_line_printer);
            PrintInfo::default_single_line()
        }
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
        let keyword = it.expect_parse()?;

        // client type: <llm>
        let client_type_node = it.expect_node_of_kind(SyntaxKind::CLIENT_TYPE)?;

        // Parse client type to get <, generic, >
        let mut ct_visitor = SyntaxNodeIter::new(client_type_node.clone());

        let rangle = ct_visitor.expect_parse()?;
        let generic = ct_visitor.expect_parse()?;
        let langle = ct_visitor.expect_parse()?;
        ct_visitor.expect_end()?;

        // name
        let name = it.expect_parse()?;

        // config block
        let config_block: ConfigBlock = it.expect_parse()?;

        it.expect_end()?;

        Ok(ClientDecl {
            keyword,
            rangle,
            generic,
            langle,
            name,
            config_block,
        })
    }
}

impl KnownKind for ClientDecl {
    fn kind() -> SyntaxKind {
        SyntaxKind::CLIENT_DEF
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
    pub items: Vec<(ConfigBlockMember, Option<t::Comma>)>,
    pub close_brace: t::RBrace,
}

impl FromCST for ConfigBlock {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::CONFIG_BLOCK)?;

        let mut it = SyntaxNodeIter::new(node);

        let open_brace = it.expect_parse()?;

        let mut items = Vec::new();
        let close_brace = loop {
            let elem = it.expect_next("CONFIG_ITEM, TYPE_BUILDER_BLOCK, or R_BRACE")?;

            let item = match elem.kind() {
                SyntaxKind::R_BRACE => break t::RBrace::from_cst(elem)?,
                SyntaxKind::CONFIG_ITEM => ConfigBlockMember::Item(ConfigItem::from_cst(elem)?),
                SyntaxKind::TYPE_BUILDER_BLOCK => {
                    ConfigBlockMember::TypeBuilder(TypeBuilderBlock::from_cst(elem)?)
                }
                _ => {
                    return Err(StrongAstError::UnexpectedKindDesc {
                        expected_desc: "CONFIG_ITEM, TYPE_BUILDER_BLOCK, or R_BRACE".into(),
                        found: elem.kind(),
                        at: elem.text_range(),
                    });
                }
            };
            let comma = it
                .next_if_kind(SyntaxKind::COMMA)
                .map(t::Comma::from_cst)
                .transpose()?;

            items.push((item, comma));
        };
        Ok(ConfigBlock {
            open_brace,
            items,
            close_brace,
        })
    }
}

impl KnownKind for ConfigBlock {
    fn kind() -> SyntaxKind {
        SyntaxKind::CONFIG_BLOCK
    }
}

impl Printable for ConfigBlock {
    /// [`ConfigBlock`] prints multi-line unless empty.
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        if self.items.is_empty() {
            printer.print_raw_token(&self.open_brace);
            printer.print_raw_token(&self.close_brace);
            return PrintInfo::default_single_line();
        }

        let inner_indent = shape.indent + printer.config.indent_width;
        let inner_shape = Shape {
            width: printer.config.line_width.saturating_sub(inner_indent),
            indent: inner_indent,
            first_line_offset: 0,
        };

        printer.print_raw_token(&self.open_brace);
        printer.print_newline();

        for (item, comma) in &self.items {
            printer.print_spaces(inner_shape.indent);
            match item {
                ConfigBlockMember::Item(item) => printer.print(item, inner_shape.clone()),
                ConfigBlockMember::TypeBuilder(block) => printer.print(block, inner_shape.clone()),
            };
            if let Some(comma) = comma {
                printer.print_raw_token(comma);
            } else {
                printer.print_str(",");
            }
            printer.print_newline();
        }

        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_brace);

        PrintInfo::default_multi_lined()
    }
}

#[derive(Debug)]
pub enum ConfigBlockMember {
    Item(ConfigItem),
    TypeBuilder(TypeBuilderBlock),
}

/// Corresponds to a [`SyntaxKind::CONFIG_ITEM`] node.
#[derive(Debug)]
pub struct ConfigItem {
    pub key: ConfigItemKey,
    // /// Colons are currently invalid, it seems
    // pub colon: Option<t::Colon>,
    pub value: ConfigItemValue,
}

impl FromCST for ConfigItem {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::CONFIG_ITEM)?;

        let mut it = SyntaxNodeIter::new(node);

        let key = it.expect_next("a CONFIG_ITEM key")?;
        let key = ConfigItemKey::from_cst(key)?;

        // let colon = it
        //     .next_if_kind(SyntaxKind::COLON)
        //     .map(|elem| {
        //         let colon = StrongAstError::assert_is_token(elem)?;
        //         Ok(t::Colon::new_from_span(colon.text_range()))
        //     })
        //     .transpose()?;

        let value = it.expect_next("a config value")?;
        let value = ConfigItemValue::from_cst(value)?;

        it.expect_end()?;

        Ok(ConfigItem {
            key,
            // colon,
            value,
        })
    }
}

impl KnownKind for ConfigItem {
    fn kind() -> SyntaxKind {
        SyntaxKind::CONFIG_ITEM
    }
}

impl Printable for ConfigItem {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        multi_lined |= printer.print(&self.key, shape.clone()).multi_lined;
        printer.print_str(" ");
        let remaining_width = printer.current_line_remaining_width();
        let value_shape = Shape {
            width: remaining_width.saturating_sub(const { ",".len() }),
            indent: shape.indent,
            first_line_offset: remaining_width.saturating_sub(shape.indent),
        };
        multi_lined |= printer.print(&self.value, value_shape).multi_lined;
        PrintInfo { multi_lined }
    }
}

#[derive(Debug)]
pub enum ConfigItemKey {
    Word(t::Word),
    String(t::QuotedString),
    RetryPolicy(t::RetryPolicy),
}

impl ConfigItemKey {
    pub fn span(&self) -> TextRange {
        match self {
            ConfigItemKey::Word(word) => word.span(),
            ConfigItemKey::String(string) => string.span(),
            ConfigItemKey::RetryPolicy(retry_policy) => retry_policy.span(),
        }
    }
}

impl FromCST for ConfigItemKey {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        match elem.kind() {
            SyntaxKind::WORD => t::Word::from_cst(elem).map(ConfigItemKey::Word),
            SyntaxKind::STRING_LITERAL => {
                t::QuotedString::from_cst(elem).map(ConfigItemKey::String)
            }
            SyntaxKind::KW_RETRY_POLICY => {
                t::RetryPolicy::from_cst(elem).map(ConfigItemKey::RetryPolicy)
            }
            _ => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "WORD or KW_RETRY_POLICY".into(),
                found: elem.kind(),
                at: elem.text_range(),
            }),
        }
    }
}

impl Printable for ConfigItemKey {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            ConfigItemKey::Word(word) => {
                printer.print_raw_token(word);
                PrintInfo::default_single_line()
            }
            ConfigItemKey::String(string) => printer.print(string, shape),
            ConfigItemKey::RetryPolicy(retry_policy) => {
                printer.print_raw_token(retry_policy);
                PrintInfo::default_single_line()
            }
        }
    }
}

/// Does not correspond to a specific [`SyntaxKind`].
#[derive(Debug)]
pub enum ConfigItemValue {
    Value(Expression),
    ConfigArray(ConfigArray),
    ConfigBlock(ConfigBlock),
}

impl FromCST for ConfigItemValue {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        match node.kind() {
            SyntaxKind::CONFIG_VALUE => {
                let mut it = SyntaxNodeIter::new(node);
                let expr = it.expect_next("an expression")?;
                if expr.kind() == SyntaxKind::ARRAY_LITERAL {
                    let array = ConfigArray::from_cst(expr)?;
                    it.expect_end()?;
                    Ok(ConfigItemValue::ConfigArray(array))
                } else {
                    let value = Expression::from_cst(expr)?;
                    it.expect_end()?; // multi-word unquoted strings are not valid in the new engine
                    Ok(ConfigItemValue::Value(value))
                }
            }
            SyntaxKind::CONFIG_BLOCK => {
                let block = ConfigBlock::from_cst(SyntaxElement::Node(node))?;
                Ok(ConfigItemValue::ConfigBlock(block))
            }
            _ => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "CONFIG_VALUE or CONFIG_BLOCK".into(),
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
            ConfigItemValue::ConfigBlock(block) => block.print(shape, printer),
            ConfigItemValue::ConfigArray(array) => array.print(shape, printer),
        }
    }
}

/// Corresponds to a [`SyntaxKind::ARRAY_LITERAL`] node, when inside a [`ConfigBlock`].
/// This is a special case because all elements will be [`ConfigItemValue`]s.
#[derive(Debug)]
pub struct ConfigArray {
    pub open_bracket: t::LBracket,
    pub elements: Vec<(ConfigItemValue, Option<t::Comma>)>,
    pub close_bracket: t::RBracket,
}

impl FromCST for ConfigArray {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::ARRAY_LITERAL)?;

        let mut it = SyntaxNodeIter::new(node);

        let open_bracket = it.expect_parse()?;

        let mut elements = Vec::new();
        let close_bracket = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing(SyntaxKind::R_BRACKET, it.parent));
            };
            match elem.kind() {
                SyntaxKind::R_BRACKET => {
                    break t::RBracket::from_cst(elem)?;
                }
                _ => {
                    let next = ConfigItemValue::from_cst(elem)?;
                    let comma = it
                        .next_if_kind(SyntaxKind::COMMA)
                        .map(t::Comma::from_cst)
                        .transpose()?;
                    elements.push((next, comma));
                }
            }
        };

        it.expect_end()?;

        Ok(ConfigArray {
            open_bracket,
            elements,
            close_bracket,
        })
    }
}

impl PrintMultiLine for ConfigArray {
    /// Multi-line layout: each element on its own indented line with trailing comma.
    /// Brackets wrap the entire construct.
    ///
    /// ```baml
    /// [
    ///     some_long_expression,
    ///     another_expression,
    /// ]
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };

        printer.print_raw_token(&self.open_bracket);
        printer.print_newline();

        for (elem, comma) in &self.elements {
            printer.print_spaces(inner_shape.indent);
            printer.print(elem, inner_shape.clone());
            if let Some(comma) = comma {
                printer.print_raw_token(comma);
            } else {
                printer.print_str(",");
            }
            printer.print_newline();
        }

        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_bracket);
        PrintInfo::default_multi_lined()
    }
}

impl Printable for ConfigArray {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        let mut single_line_printer =
            Printer::new_empty(printer.input, printer.config, printer.trivia);
        single_line_printer.print_raw_token(&self.open_bracket);
        for (i, (elem, comma)) in self.elements.iter().enumerate() {
            multi_lined |= single_line_printer
                .print(elem, Shape::unlimited_single_line())
                .multi_lined;
            if i + 1 < self.elements.len() {
                if let Some(comma) = comma {
                    single_line_printer.print_raw_token(comma);
                } else {
                    single_line_printer.print_str(",");
                }
                single_line_printer.print_str(" ");
            }
            if multi_lined || single_line_printer.output.len() > shape.width {
                return Self::print_multi_line(self, shape, printer);
            }
        }
        single_line_printer.print_raw_token(&self.close_bracket);

        if multi_lined || single_line_printer.output.len() > shape.width {
            Self::print_multi_line(self, shape, printer)
        } else {
            printer.append_from_printer(single_line_printer);
            PrintInfo::default_single_line()
        }
    }
}

/// Corresponds to a [`SyntaxKind::TYPE_BUILDER_BLOCK`] node.
#[derive(Debug)]
pub struct TypeBuilderBlock {
    pub keyword: t::TypeBuilder,
    pub open_brace: t::LBrace,
    pub items: Vec<TypeBuilderItem>,
    pub close_brace: t::RBrace,
}

impl FromCST for TypeBuilderBlock {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::TYPE_BUILDER_BLOCK)?;

        let mut it = SyntaxNodeIter::new(node);

        let keyword = it.expect_parse()?;

        let open_brace = it.expect_parse()?;

        let mut items = Vec::new();
        let close_brace = loop {
            let elem = it.expect_next("DYNAMIC_TYPE_DEF, CLASS_DEF, or ENUM_DEF")?;
            if elem.kind() == SyntaxKind::R_BRACE {
                break t::RBrace::from_cst(elem)?;
            }

            items.push(TypeBuilderItem::from_cst(elem)?);
        };

        it.expect_end()?;

        Ok(TypeBuilderBlock {
            keyword,
            open_brace,
            items,
            close_brace,
        })
    }
}

impl KnownKind for TypeBuilderBlock {
    fn kind() -> SyntaxKind {
        SyntaxKind::TYPE_BUILDER_BLOCK
    }
}

impl Printable for TypeBuilderBlock {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };

        printer.print_raw_token(&self.keyword);
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
pub enum TypeBuilderItem {
    /// Corresponds to a [`SyntaxKind::DYNAMIC_TYPE_DEF`] node that containins a class definition.
    DynamicClass(t::Dynamic, ClassDecl),
    /// Corresponds to a [`SyntaxKind::DYNAMIC_TYPE_DEF`] node that containins an enum definition.
    DynamicEnum(t::Dynamic, EnumDecl),
    Class(ClassDecl),
    Enum(EnumDecl),
}

impl FromCST for TypeBuilderItem {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        match elem.kind() {
            SyntaxKind::DYNAMIC_TYPE_DEF => {
                let node = StrongAstError::assert_is_node(elem)?;
                let mut it = SyntaxNodeIter::new(node);
                let dynamic = it.expect_parse()?;
                let class_or_enum = it.expect_next("CLASS_DEF or ENUM_DEF")?;
                match class_or_enum.kind() {
                    SyntaxKind::CLASS_DEF => {
                        let class = ClassDecl::from_cst(class_or_enum)?;
                        it.expect_end()?;
                        Ok(TypeBuilderItem::DynamicClass(dynamic, class))
                    }
                    SyntaxKind::ENUM_DEF => {
                        let enum_def = EnumDecl::from_cst(class_or_enum)?;
                        it.expect_end()?;
                        Ok(TypeBuilderItem::DynamicEnum(dynamic, enum_def))
                    }
                    _ => Err(StrongAstError::UnexpectedKindDesc {
                        expected_desc: "CLASS_DEF or ENUM_DEF".into(),
                        found: class_or_enum.kind(),
                        at: class_or_enum.text_range(),
                    }),
                }
            }
            SyntaxKind::CLASS_DEF => {
                let class = ClassDecl::from_cst(elem)?;
                Ok(TypeBuilderItem::Class(class))
            }
            SyntaxKind::ENUM_DEF => {
                let enum_def = EnumDecl::from_cst(elem)?;
                Ok(TypeBuilderItem::Enum(enum_def))
            }
            _ => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "DYNAMIC_TYPE_DEF, CLASS_DEF, or ENUM_DEF".into(),
                found: elem.kind(),
                at: elem.text_range(),
            }),
        }
    }
}

impl Printable for TypeBuilderItem {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            TypeBuilderItem::DynamicClass(dynamic, class) => {
                printer.print_raw_token(dynamic);
                printer.print_str(" ");
                printer.print(class, shape)
            }
            TypeBuilderItem::DynamicEnum(dynamic, enum_def) => {
                printer.print_raw_token(dynamic);
                printer.print_str(" ");
                printer.print(enum_def, shape)
            }
            TypeBuilderItem::Class(class) => printer.print(class, shape),
            TypeBuilderItem::Enum(enum_def) => printer.print(enum_def, shape),
        }
    }
}

#[derive(Debug)]
pub struct TestDecl {
    pub keyword: t::Test,
    pub name: t::Word,
    pub config_block: ConfigBlock,
}

impl FromCST for TestDecl {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::TEST_DEF)?;

        let mut it = SyntaxNodeIter::new(node);

        // keyword: "test"
        let keyword = it.expect_parse()?;

        // name
        let name = it.expect_parse()?;

        // config block
        let config_block: ConfigBlock = it.expect_parse()?;

        it.expect_end()?;

        Ok(TestDecl {
            keyword,
            name,
            config_block,
        })
    }
}

impl KnownKind for TestDecl {
    fn kind() -> SyntaxKind {
        SyntaxKind::TEST_DEF
    }
}

impl Printable for TestDecl {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print_raw_token(&self.name);
        printer.print_str(" ");
        printer.print(&self.config_block, shape);
        PrintInfo::default_multi_lined()
    }
}

#[derive(Debug)]
pub struct RetryPolicyDecl {
    pub keyword: t::RetryPolicy,
    pub name: t::Word,
    pub config_block: ConfigBlock,
}

impl FromCST for RetryPolicyDecl {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::RETRY_POLICY_DEF)?;

        let mut it = SyntaxNodeIter::new(node);

        // keyword: "retry_policy"
        let keyword = it.expect_parse()?;

        // name
        let name = it.expect_parse()?;

        // config block
        let config_block: ConfigBlock = it.expect_parse()?;

        it.expect_end()?;

        Ok(RetryPolicyDecl {
            keyword,
            name,
            config_block,
        })
    }
}

impl KnownKind for RetryPolicyDecl {
    fn kind() -> SyntaxKind {
        SyntaxKind::RETRY_POLICY_DEF
    }
}

impl Printable for RetryPolicyDecl {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print_raw_token(&self.name);
        printer.print_str(" ");
        printer.print(&self.config_block, shape)
    }
}

#[derive(Debug)]
pub struct TemplateStringDecl {
    pub keyword: t::TemplateString,
    pub name: t::Word,
    pub args: FunctionParamList,
    pub raw_string: t::RawString,
}

impl FromCST for TemplateStringDecl {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::TEMPLATE_STRING_DEF)?;

        let mut it = SyntaxNodeIter::new(node);

        // keyword: "template_string"
        let keyword = it.expect_parse()?;

        // name
        let name = it.expect_parse()?;

        // args
        let args: FunctionParamList = it.expect_parse()?;

        // raw string
        let raw_string: t::RawString = it.expect_parse()?;

        it.expect_end()?;

        Ok(TemplateStringDecl {
            keyword,
            name,
            args,
            raw_string,
        })
    }
}

impl KnownKind for TemplateStringDecl {
    fn kind() -> SyntaxKind {
        SyntaxKind::TEMPLATE_STRING_DEF
    }
}

impl Printable for TemplateStringDecl {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;

        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print_raw_token(&self.name);
        printer.print_str(" ");
        multi_lined |= printer.print(&self.args, shape.clone()).multi_lined;
        printer.print_str(" ");
        multi_lined |= printer
            .print(&self.raw_string, Shape::unlimited_single_line())
            .multi_lined;
        PrintInfo { multi_lined }
    }
}

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
        let keyword = it.expect_parse()?;

        // name
        let name = it.expect_parse()?;

        // equals
        let equals = it.expect_parse()?;

        // type expression
        let type_expr: Type = it.expect_parse()?;

        // optional semicolon
        let semicolon = it.next().map(t::Semicolon::from_cst).transpose()?;

        it.expect_end()?;

        Ok(TypeAliasDecl {
            keyword,
            name,
            equals,
            type_expr,
            semicolon,
        })
    }
}

impl KnownKind for TypeAliasDecl {
    fn kind() -> SyntaxKind {
        SyntaxKind::TYPE_ALIAS_DEF
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
