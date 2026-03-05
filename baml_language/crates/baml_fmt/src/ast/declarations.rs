use baml_db::baml_compiler_syntax::{SyntaxElement, SyntaxKind};
use rowan::TextRange;

use crate::{
    EmittableTrivia,
    ast::{
        Attribute, BlockAttribute, BlockExpr, Expression, FromCST, KnownKind, PathExpr,
        StrongAstError, SyntaxNodeIter, Token, Type, tokens as t,
    },
    printer::{PrintInfo, PrintMultiLine, Printable, Printer, Shape},
    trivia_classifier::TriviaSliceExt as _,
};

/// Any of the valid top-level declarations in a [`super::SourceFile`].
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
    Generator(GeneratorDecl),
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
            SyntaxKind::GENERATOR_DEF => {
                TopLevelDeclaration::Generator(GeneratorDecl::from_cst(elem)?)
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
            TopLevelDeclaration::Generator(generator_decl) => generator_decl.print(shape, printer),
            TopLevelDeclaration::Unknown(range) => {
                // May not be idempotent due to whitespace changes, but that's okay because we shouldn't
                // have unknown stuff anyway.
                printer.print_input_range(*range);
                PrintInfo::default_multi_lined()
            }
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            TopLevelDeclaration::Function(f) => f.leftmost_token(),
            TopLevelDeclaration::Class(c) => c.leftmost_token(),
            TopLevelDeclaration::Enum(e) => e.leftmost_token(),
            TopLevelDeclaration::Client(c) => c.leftmost_token(),
            TopLevelDeclaration::Test(t) => t.leftmost_token(),
            TopLevelDeclaration::RetryPolicy(r) => r.leftmost_token(),
            TopLevelDeclaration::TemplateString(t) => t.leftmost_token(),
            TopLevelDeclaration::TypeAlias(t) => t.leftmost_token(),
            TopLevelDeclaration::Generator(g) => g.leftmost_token(),
            TopLevelDeclaration::Unknown(range) => *range,
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            TopLevelDeclaration::Function(f) => f.rightmost_token(),
            TopLevelDeclaration::Class(c) => c.rightmost_token(),
            TopLevelDeclaration::Enum(e) => e.rightmost_token(),
            TopLevelDeclaration::Client(c) => c.rightmost_token(),
            TopLevelDeclaration::Test(t) => t.rightmost_token(),
            TopLevelDeclaration::RetryPolicy(r) => r.rightmost_token(),
            TopLevelDeclaration::TemplateString(t) => t.rightmost_token(),
            TopLevelDeclaration::TypeAlias(t) => t.rightmost_token(),
            TopLevelDeclaration::Generator(g) => g.rightmost_token(),
            TopLevelDeclaration::Unknown(range) => *range,
        }
    }
}

/// Corresponds to a [`SyntaxKind::FUNCTION_DEF`] node.
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

        let mut it = SyntaxNodeIter::new(&node);

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
        let (_, return_type_line_comment) =
            return_type_printer.print_trivia_all_trailing_for(self.return_type.rightmost_token());

        let single_line_size = printer.current_line_len()
            + param_printer.output.len()
            + const { " -> ".len() + " {".len() }
            + return_type_printer.output.len();
        if single_line_size <= printer.config.line_width
            && !param_info.multi_lined
            && !return_type_info.multi_lined
            && !return_type_line_comment
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

            // Trivia between -> and return type
            let (_, arrow_trailing) = printer.trivia.get_for_range_split(self.arrow.span());
            printer.print_trivia_squished(arrow_trailing);
            let return_type_leading = printer.trivia.get_leading_for_element(&self.return_type);
            printer.print_trivia_squished(return_type_leading);

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
            let (_, return_type_line_comment) =
                printer.print_trivia_all_trailing_for(self.return_type.rightmost_token());

            if (return_info.multi_lined && self.return_type.multi_line_is_indented())
                || return_type_line_comment
            {
                // `{` goes on its own line after the type ends
                printer.print_newline();
            } else {
                printer.print_str(" ");
            }

            printer.print(&self.body, shape);

            PrintInfo::default_multi_lined()
        }
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.body.rightmost_token()
    }
}

/// Corresponds to a [`SyntaxKind::PARAMETER_LIST`] node.
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

        let mut it = SyntaxNodeIter::new(&node);

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
        let inner_indent = shape.indent + printer.config.indent_width;
        let inner_shape = Shape {
            width: printer.config.line_width.saturating_sub(inner_indent),
            indent: inner_indent,
            first_line_offset: 0,
        };

        printer.print_raw_token(&self.open_paren);
        printer.print_trivia_all_trailing_for(self.open_paren.span());
        printer.print_newline();

        for (param, comma) in &self.params {
            let (param_leading, param_trailing) = printer.trivia.get_for_element(param);
            printer.print_trivia_with_newline(param_leading.trim_blanks(), inner_shape.indent);
            printer.print_spaces(inner_shape.indent);
            printer.print(param, inner_shape.clone());
            if let Some(comma) = comma {
                printer.print_trivia_squished(param_trailing);
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(comma.span());
                printer.print_trivia_squished(comma_leading);
                printer.print_raw_token(comma);
                printer.print_trivia_trailing(comma_trailing);
            } else {
                printer.print_str(",");
                printer.print_trivia_trailing(param_trailing);
            }
            printer.print_newline();
        }

        let (close_paren_leading, _) = printer.trivia.get_for_range_split(self.close_paren.span());
        printer.print_trivia_with_newline(close_paren_leading.trim_blanks(), inner_shape.indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_paren);
        PrintInfo::default_multi_lined()
    }
}

impl FunctionParamList {
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the function param list on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.open_paren);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_paren.span());
        printer.try_print_trivia_single_line_squished(open_trailing)?;

        for (i, (param, comma)) in self.params.iter().enumerate() {
            if printer.output.len() > shape.width {
                return None;
            }
            let (p_leading, p_trailing) = printer.trivia.get_for_element(param);
            printer.try_print_trivia_single_line_squished(p_leading)?;
            if printer
                .print(param, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }

            let (comma_leading, comma_trailing) = if let Some(comma) = comma {
                printer.trivia.get_for_range_split(comma.span())
            } else {
                (&[][..], &[][..])
            };
            if i + 1 < self.params.len() {
                printer.print_trivia_squished(p_trailing);
                printer.print_trivia_squished(comma_leading);
                printer.print_str(", ");
                printer.try_print_trivia_single_line_squished(comma_trailing)?;
            } else {
                // Trailing comma is removed in single-line mode, but we still try the comments.
                printer.try_print_trivia_single_line_squished(p_trailing)?;
                printer.try_print_trivia_single_line_squished(comma_leading)?;
                printer.try_print_trivia_single_line_squished(comma_trailing)?;
            }
        }

        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_paren.span());
        printer.try_print_trivia_single_line_squished(close_leading)?;
        printer.print_raw_token(&self.close_paren);

        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl Printable for FunctionParamList {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.open_paren.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_paren.span()
    }
}

/// Corresponds to a [`SyntaxKind::PARAMETER`] node.
#[derive(Debug)]
pub struct FunctionParam {
    pub name: t::Word,
    pub ty: Option<(t::Colon, Type)>,
}

impl FromCST for FunctionParam {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::PARAMETER)?;

        let mut it = SyntaxNodeIter::new(&node);

        let name = it.expect_parse()?;

        let ty = if let Some(colon_elem) = it.next_if_kind(SyntaxKind::COLON) {
            let colon = t::Colon::from_cst(colon_elem)?;
            Some((colon, it.expect_parse()?))
        } else {
            // No type annotation (e.g. `self`)
            None
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
            let mut trivia_len = 0;
            let (_, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
            printer.print_str(": ");
            trivia_len += printer.print_trivia_squished(colon_trailing);
            let ty_leading = printer.trivia.get_leading_for_element(ty);
            trivia_len += printer.print_trivia_squished(ty_leading);

            let new_offset = usize::from(self.name.span().len()) + 2 + trivia_len;
            let ty_shape = Shape {
                width: shape.width.saturating_sub(new_offset),
                indent: shape.indent,
                first_line_offset: shape.first_line_offset + new_offset,
            };
            ty.print(ty_shape, printer)
        } else {
            PrintInfo::default_single_line()
        }
    }
    fn leftmost_token(&self) -> TextRange {
        self.name.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.ty
            .as_ref()
            .map_or(self.name.span(), |(_, ty)| ty.rightmost_token())
    }
}

/// Any of the valid function bodies in a [`FunctionDecl`].
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
                let mut visitor = SyntaxNodeIter::new(&node);
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
    fn leftmost_token(&self) -> TextRange {
        match self {
            FunctionDeclBody::Llm(llm) => llm.leftmost_token(),
            FunctionDeclBody::Block(block) => block.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            FunctionDeclBody::Llm(llm) => llm.rightmost_token(),
            FunctionDeclBody::Block(block) => block.rightmost_token(),
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

        let mut it = SyntaxNodeIter::new(&node);

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

        Ok(LlmFunctionBody {
            open_brace,
            client,
            prompt,
            close_brace,
        })
    }
}

impl KnownKind for LlmFunctionBody {
    fn kind() -> SyntaxKind {
        SyntaxKind::LLM_FUNCTION_BODY
    }
}

impl Printable for LlmFunctionBody {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_indent = shape.indent + printer.config.indent_width;

        printer.print_raw_token(&self.open_brace);
        printer.print_trivia_all_trailing_for(self.open_brace.span());
        printer.print_newline();

        let (client_leading, client_trailing) = printer.trivia.get_for_element(&self.client);
        printer.print_trivia_with_newline(client_leading.trim_leading_blanks(), inner_indent);
        printer.print_spaces(inner_indent);
        let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
        self.client.print(inner_shape, printer);
        printer.print_trivia_trailing(client_trailing);
        printer.print_newline();

        printer.print_standalone_with_trivia(&self.prompt, inner_indent);
        printer.print_newline();

        let (close_brace_leading, _) = printer.trivia.get_for_range_split(self.close_brace.span());
        printer.print_trivia_with_newline(close_brace_leading.trim_trailing_blanks(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_brace);

        PrintInfo::default_multi_lined()
    }
    fn leftmost_token(&self) -> TextRange {
        self.open_brace.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_brace.span()
    }
}

/// Corresponds to a [`SyntaxKind::CLIENT_FIELD`] node.
#[derive(Debug)]
pub struct ClientField {
    pub keyword: t::Client,
    pub colon: Option<t::Colon>,
    pub name: ClientName,
}

impl FromCST for ClientField {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::CLIENT_FIELD)?;

        let mut it = SyntaxNodeIter::new(&node);

        let keyword = it.expect_parse()?;

        let colon = it
            .next_if_kind(SyntaxKind::COLON)
            .map(t::Colon::from_cst)
            .transpose()?;

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
                    found,
                    at: name.text_range(),
                });
            }
        };

        it.expect_end()?;

        Ok(ClientField {
            keyword,
            colon,
            name,
        })
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
        let colon_trailing = if let Some(colon) = &self.colon {
            let (_, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
            colon_trailing
        } else {
            &[][..]
        };
        printer.print_str(": ");
        printer.print_trivia_squished(colon_trailing);
        let name_leading = printer.trivia.get_leading_for_element(&self.name);
        printer.print_trivia_squished(name_leading);
        printer.print(&self.name, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.name.rightmost_token()
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
    fn leftmost_token(&self) -> TextRange {
        match self {
            ClientName::Path(path) => path.leftmost_token(),
            ClientName::String(string) => string.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            ClientName::Path(path) => path.rightmost_token(),
            ClientName::String(string) => string.rightmost_token(),
        }
    }
}

/// Corresponds to a [`SyntaxKind::PROMPT_FIELD`] node.
#[derive(Debug)]
pub struct PromptField {
    pub prompt: t::Word,
    pub colon: Option<t::Colon>,
    pub string: PromptValue,
}

impl FromCST for PromptField {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::PROMPT_FIELD)?;

        let mut it = SyntaxNodeIter::new(&node);

        // It's a word, but we should never be in a `PROMPT_FIELD` context if it's not a prompt
        let prompt = it.expect_parse()?;

        let colon = it
            .next_if_kind(SyntaxKind::COLON)
            .map(t::Colon::from_cst)
            .transpose()?;

        let string = it.expect_next("a prompt string")?;
        let string = match string.kind() {
            SyntaxKind::RAW_STRING_LITERAL => {
                PromptValue::RawString(t::RawString::from_cst(string)?)
            }
            SyntaxKind::STRING_LITERAL => PromptValue::String(t::QuotedString::from_cst(string)?),
            _ => {
                return Err(StrongAstError::UnexpectedKindDesc {
                    expected_desc: "STRING_LITERAL or RAW_STRING_LITERAL".into(),
                    found: string.kind(),
                    at: string.text_range(),
                });
            }
        };

        it.expect_end()?;

        Ok(PromptField {
            prompt,
            colon,
            string,
        })
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
        let colon_trailing = if let Some(colon) = &self.colon {
            let (_, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
            colon_trailing
        } else {
            &[][..]
        };
        printer.print_str(": ");
        printer.print_trivia_squished(colon_trailing);
        let string_leading = printer.trivia.get_leading_for_element(&self.string);
        printer.print_trivia_squished(string_leading);
        printer.print(&self.string, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.prompt.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.string.rightmost_token()
    }
}

/// Any of the valid values in a [`PromptField`].
#[derive(Debug)]
pub enum PromptValue {
    RawString(t::RawString),
    String(t::QuotedString),
}

impl Printable for PromptValue {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            PromptValue::RawString(raw_string) => printer.print(raw_string, shape),
            PromptValue::String(string) => printer.print(string, shape),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            PromptValue::RawString(raw_string) => raw_string.leftmost_token(),
            PromptValue::String(string) => string.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            PromptValue::RawString(raw_string) => raw_string.rightmost_token(),
            PromptValue::String(string) => string.rightmost_token(),
        }
    }
}

/// Corresponds to a [`SyntaxKind::CLASS_DEF`] node.
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

        let mut it = SyntaxNodeIter::new(&node);

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
        let inner_indent = shape.indent + printer.config.indent_width;

        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print_raw_token(&self.name);
        printer.print_str(" ");
        printer.print_raw_token(&self.open_brace);
        printer.print_trivia_all_trailing_for(self.open_brace.span());
        printer.print_newline();

        if let Some((first, rest)) = self.items.split_first() {
            // first has leading empty lines trimmed
            let (first_leading, first_trailing) = printer.trivia.get_for_element(first);
            printer.print_trivia_with_newline(first_leading.trim_leading_blanks(), inner_indent);
            printer.print_spaces(inner_indent);
            let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
            first.print(inner_shape, printer);
            printer.print_trivia_trailing(first_trailing);
            printer.print_newline();

            // rest can have leading empty lines
            for item in rest {
                printer.print_standalone_with_trivia(item, inner_indent);
                printer.print_newline();
            }
        }

        let (close_brace_leading, _) = printer.trivia.get_for_range_split(self.close_brace.span());
        printer.print_trivia_with_newline(close_brace_leading.trim_trailing_blanks(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_brace);

        PrintInfo::default_multi_lined()
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_brace.span()
    }
}

#[derive(Debug)]
pub struct ClassField {
    pub name: t::Word,
    pub colon: Option<t::Colon>,
    pub ty: Type,
    pub attributes: Vec<Attribute>,
    // pub comma: Option<t::Comma>,
}

impl FromCST for ClassField {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::FIELD)?;

        let mut it = SyntaxNodeIter::new(&node);

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
        for attr in it {
            attributes.push(Attribute::from_cst(attr)?);
        }

        Ok(ClassField {
            name,
            colon,
            ty,
            attributes,
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
    /// below the field name and type. Trailing comments on the type are preserved.
    ///
    /// ```baml
    /// myField ReallyLongTypeName // trailing comment
    ///     @alias("theLongField")
    ///     @description("some desc")
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let attr_shape = Shape::standalone(
            printer.config.line_width,
            shape.indent + printer.config.indent_width,
        );

        printer.print_raw_token(&self.name);
        let colon_trailing = if let Some(colon) = &self.colon {
            let (_, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
            colon_trailing
        } else {
            &[][..]
        };
        printer.print_str(": ");
        printer.print_trivia_squished(colon_trailing);

        let (type_leading, type_trailing) = printer.trivia.get_for_element(&self.ty);
        printer.print_trivia_squished(type_leading);
        printer.print(&self.ty, shape);
        if !self.attributes.is_empty() {
            // we have attributes, they will be on their own lines so we can print the trailing trivia
            printer.print_trivia_trailing(type_trailing);
        }

        for (i, attr) in self.attributes.iter().enumerate() {
            let (attr_leading, attr_trailing) = printer.trivia.get_for_element(attr);
            printer.print_newline();
            printer.print_trivia_with_newline(attr_leading.trim_blanks(), attr_shape.indent);
            printer.print_spaces(attr_shape.indent);
            printer.print(attr, attr_shape.clone());
            let is_last = i + 1 >= self.attributes.len();
            if !is_last {
                // we have more attributes, so we can print the trailing trivia
                printer.print_trivia_trailing(attr_trailing);
            }
        }

        PrintInfo::default_multi_lined()
    }
}

impl ClassField {
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the class field on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.name);
        let colon_trailing = if let Some(colon) = &self.colon {
            let (_, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
            colon_trailing
        } else {
            &[][..]
        };
        printer.print_str(": ");
        printer.try_print_trivia_single_line_squished(colon_trailing)?;

        let (type_leading, type_trailing) = printer.trivia.get_for_element(&self.ty);
        printer.print_trivia_squished(type_leading);
        if self
            .ty
            .print(Shape::unlimited_single_line(), printer)
            .multi_lined
            || printer.len() > shape.width
        {
            return None;
        }
        if !self.attributes.is_empty() {
            // type is not the last element
            printer.try_print_trivia_single_line_squished(type_trailing)?;
        }

        for (i, attr) in self.attributes.iter().enumerate() {
            printer.print_str(" ");
            let (attr_leading, attr_trailing) = printer.trivia.get_for_element(attr);
            printer.try_print_trivia_single_line_squished(attr_leading)?;
            if printer
                .print(attr, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
            let is_last = i + 1 >= self.attributes.len();
            if !is_last {
                // not last, we could take up the rest of the line if multilined
                printer.try_print_trivia_single_line_squished(attr_trailing)?;
            }
        }

        if printer.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl Printable for ClassField {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.name.span()
    }
    fn rightmost_token(&self) -> TextRange {
        if let Some(attr) = self.attributes.last() {
            return attr.rightmost_token();
        }
        self.ty.rightmost_token()
    }
}

/// Any of the valid items in a [`ClassDecl`].
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
    fn leftmost_token(&self) -> TextRange {
        match self {
            ClassItem::Field(field) => field.leftmost_token(),
            ClassItem::Function(function) => function.leftmost_token(),
            ClassItem::BlockAttribute(attr) => attr.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            ClassItem::Field(field) => field.rightmost_token(),
            ClassItem::Function(function) => function.rightmost_token(),
            ClassItem::BlockAttribute(attr) => attr.rightmost_token(),
        }
    }
}

/// Corresponds to a [`SyntaxKind::ENUM_DEF`] node.
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
        let mut it = SyntaxNodeIter::new(&node);

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
        let inner_indent = shape.indent + printer.config.indent_width;

        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print_raw_token(&self.name);
        printer.print_str(" ");
        printer.print_raw_token(&self.open_brace);
        printer.print_trivia_all_trailing_for(self.open_brace.span());
        printer.print_newline();

        if let Some((first, rest)) = self.items.split_first() {
            // first has leading empty lines trimmed
            let (first_leading, first_trailing) = printer.trivia.get_for_element(first);
            printer.print_trivia_with_newline(first_leading.trim_leading_blanks(), inner_indent);
            printer.print_spaces(inner_indent);
            let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
            first.print(inner_shape, printer);
            printer.print_trivia_trailing(first_trailing);
            printer.print_newline();

            // rest can have leading empty lines
            for item in rest {
                printer.print_standalone_with_trivia(item, inner_indent);
                printer.print_newline();
            }
        }

        let (close_brace_leading, _) = printer.trivia.get_for_range_split(self.close_brace.span());
        printer.print_trivia_with_newline(close_brace_leading.trim_trailing_blanks(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_brace);

        PrintInfo::default_multi_lined()
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_brace.span()
    }
}

/// Any of the valid items in an [`EnumDecl`].
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
    fn leftmost_token(&self) -> TextRange {
        match self {
            EnumItem::Variant(variant, _) => variant.leftmost_token(),
            EnumItem::BlockAttribute(attr) => attr.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            EnumItem::Variant(variant, comma) => {
                if let Some(comma) = comma {
                    comma.span()
                } else {
                    variant.rightmost_token()
                }
            }
            EnumItem::BlockAttribute(attr) => attr.rightmost_token(),
        }
    }
}

/// Corresponds to a [`SyntaxKind::ENUM_VARIANT`] node.
#[derive(Debug)]
pub struct EnumVariant {
    pub name: t::Word,
    pub attributes: Vec<Attribute>,
}

impl FromCST for EnumVariant {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::ENUM_VARIANT)?;

        let mut it = SyntaxNodeIter::new(&node);

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
    /// below the variant name. Trailing comments on the name are preserved.
    ///
    /// ```baml
    /// VariantName // description
    ///     @alias("something_long")
    ///     @description("a long description")
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.name);

        if self.attributes.is_empty() {
            // you shouldn't call print_multi_line if this is the case.
            return PrintInfo::default_single_line();
        }
        printer.print_trivia_all_trailing_for(self.name.span());

        let attr_shape = Shape::standalone(
            printer.config.line_width,
            shape.indent + printer.config.indent_width,
        );
        for (i, attr) in self.attributes.iter().enumerate() {
            let (attr_leading, attr_trailing) = printer.trivia.get_for_element(attr);
            printer.print_newline();
            printer.print_trivia_with_newline(attr_leading.trim_blanks(), attr_shape.indent);
            printer.print_spaces(attr_shape.indent);
            printer.print(attr, attr_shape.clone());
            if i + 1 < self.attributes.len() {
                printer.print_trivia_trailing(attr_trailing);
            }
        }

        PrintInfo::default_multi_lined()
    }
}

impl EnumVariant {
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the enum variant on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.name);
        let (_, name_trailing) = printer.trivia.get_for_range_split(self.name.span());
        printer.try_print_trivia_single_line_squished(name_trailing)?;

        for (i, attr) in self.attributes.iter().enumerate() {
            printer.print_spaces(1);
            let (attr_leading, attr_trailing) = printer.trivia.get_for_element(attr);
            printer.try_print_trivia_single_line_squished(attr_leading)?;
            if attr
                .print(Shape::unlimited_single_line(), printer)
                .multi_lined
            {
                return None;
            }
            if i + 1 < self.attributes.len() {
                printer.try_print_trivia_single_line_squished(attr_trailing)?;
            }
        }

        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl Printable for EnumVariant {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.name.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.attributes
            .last()
            .map_or(self.name.span(), Printable::rightmost_token)
    }
}

/// Corresponds to a [`SyntaxKind::CLIENT_DEF`] node.
#[derive(Debug)]
pub struct ClientDecl {
    pub keyword: t::Client,
    pub client_type: Option<ClientType>,
    pub name: t::Word,
    pub config_block: ConfigBlock,
}

impl FromCST for ClientDecl {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::CLIENT_DEF)?;

        let mut it = SyntaxNodeIter::new(&node);

        // keyword: "client"
        let keyword = it.expect_parse()?;

        // client type: <llm>
        let client_type = it
            .next_if_kind(SyntaxKind::CLIENT_TYPE)
            .map(ClientType::from_cst)
            .transpose()?;

        // name
        let name = it.expect_parse()?;

        // config block
        let config_block: ConfigBlock = it.expect_parse()?;

        it.expect_end()?;

        Ok(ClientDecl {
            keyword,
            client_type,
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
        if let Some(client_type) = &self.client_type {
            printer.print(client_type, Shape::unlimited_single_line());
        }
        printer.print_str(" ");
        printer.print_raw_token(&self.name);
        printer.print_str(" ");
        printer.print(&self.config_block, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.config_block.rightmost_token()
    }
}

/// Corresponds to a [`SyntaxKind::CLIENT_TYPE`] node.
#[derive(Debug)]
pub struct ClientType {
    pub langle: t::Less,
    pub generic: t::Word,
    pub rangle: t::Greater,
}

impl FromCST for ClientType {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::CLIENT_TYPE)?;

        let mut it = SyntaxNodeIter::new(&node);

        let langle = it.expect_parse()?;
        let generic = it.expect_parse()?;
        let rangle = it.expect_parse()?;

        it.expect_end()?;

        Ok(ClientType {
            langle,
            generic,
            rangle,
        })
    }
}

impl KnownKind for ClientType {
    fn kind() -> SyntaxKind {
        SyntaxKind::CLIENT_TYPE
    }
}

impl Printable for ClientType {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.langle);
        printer.print_raw_token(&self.generic);
        printer.print_raw_token(&self.rangle);
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.langle.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.rangle.span()
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

        let mut it = SyntaxNodeIter::new(&node);

        let open_brace = it.expect_parse()?;

        let mut items = Vec::new();
        let close_brace = loop {
            let elem =
                it.expect_next("CONFIG_ITEM, TYPE_BUILDER_BLOCK, BLOCK_ATTRIBUTE, or R_BRACE")?;

            let item = match elem.kind() {
                SyntaxKind::R_BRACE => break t::RBrace::from_cst(elem)?,
                SyntaxKind::CONFIG_ITEM => ConfigBlockMember::Item(ConfigItem::from_cst(elem)?),
                SyntaxKind::TYPE_BUILDER_BLOCK => {
                    ConfigBlockMember::TypeBuilder(TypeBuilderBlock::from_cst(elem)?)
                }
                SyntaxKind::BLOCK_ATTRIBUTE => {
                    ConfigBlockMember::BlockAttribute(BlockAttribute::from_cst(elem)?)
                }
                _ => {
                    return Err(StrongAstError::UnexpectedKindDesc {
                        expected_desc:
                            "CONFIG_ITEM, TYPE_BUILDER_BLOCK, BLOCK_ATTRIBUTE, or R_BRACE".into(),
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

        it.expect_end()?;

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
        let inner_indent = shape.indent + printer.config.indent_width;

        if self.items.is_empty() {
            // Check if there's trivia inside the empty block (e.g. comments between { and })
            let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_brace.span());
            let (close_leading, _) = printer.trivia.get_for_range_split(self.close_brace.span());
            let has_comments = open_trailing
                .iter()
                .chain(close_leading.iter())
                .any(EmittableTrivia::is_comment);

            if has_comments {
                printer.print_raw_token(&self.open_brace);
                printer.print_trivia_trailing(open_trailing);
                printer.print_newline();
                printer.print_trivia_with_newline(close_leading.trim_blanks(), inner_indent);
                printer.print_spaces(shape.indent);
                printer.print_raw_token(&self.close_brace);
                return PrintInfo::default_multi_lined();
            }
            printer.print_raw_token(&self.open_brace);
            printer.print_raw_token(&self.close_brace);
            return PrintInfo::default_single_line();
        }

        let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);

        printer.print_raw_token(&self.open_brace);
        printer.print_trivia_all_trailing_for(self.open_brace.span());
        printer.print_newline();

        let mut block_attrs: Vec<(&BlockAttribute, &ConfigBlockMember, Option<&t::Comma>)> = self
            .items
            .iter()
            .filter_map(|(item, comma)| match item {
                ConfigBlockMember::BlockAttribute(attr) => Some((attr, item, comma.as_ref())),
                _ => None,
            })
            .collect();
        block_attrs.sort_by_cached_key(|(attr, _, _)| {
            attr.name_parts_str(printer.input).collect::<Vec<&str>>()
        });
        let other_items = self
            .items
            .iter()
            .filter(|(item, _)| !matches!(item, ConfigBlockMember::BlockAttribute(_)))
            .map(|(item, comma)| (item, comma.as_ref()));

        let ordered_items = block_attrs
            .into_iter()
            .map(|(_, member, comma)| (member, comma))
            .chain(other_items);

        for (i, (item, comma)) in ordered_items.enumerate() {
            let (item_leading, item_trailing) = printer.trivia.get_for_element(item);
            let item_leading = if i == 0 {
                item_leading.trim_leading_blanks() // this is first item
            } else {
                item_leading
            };

            printer.print_trivia_with_newline(item_leading, inner_indent);
            printer.print_spaces(inner_indent);
            printer.print(item, inner_shape.clone());

            match (item, comma) {
                (ConfigBlockMember::BlockAttribute(_), Some(comma)) => {
                    // remove the trailing comma, keep the comments
                    let (comma_leading, comma_trailing) =
                        printer.trivia.get_for_range_split(comma.span());
                    printer.print_trivia_trailing(item_trailing);
                    printer.print_trivia_trailing(comma_leading);
                    printer.print_trivia_trailing(comma_trailing);
                }
                (ConfigBlockMember::BlockAttribute(_), None) => {
                    // keep no comma, print trivia nicely
                    printer.print_trivia_trailing(item_trailing);
                }
                (_, Some(comma)) => {
                    // keep the comma, print trivia nicely
                    let (comma_leading, comma_trailing) =
                        printer.trivia.get_for_range_split(comma.span());
                    printer.print_trivia_squished(item_trailing);
                    printer.print_trivia_squished(comma_leading);
                    printer.print_raw_token(comma);
                    printer.print_trivia_trailing(comma_trailing);
                }
                (_, None) => {
                    // comma is inserted *before* the trailing trivia
                    printer.print_str(",");
                    printer.print_trivia_trailing(item_trailing);
                }
            }
            printer.print_newline();
        }

        let (close_brace_leading, _) = printer.trivia.get_for_range_split(self.close_brace.span());
        printer.print_trivia_with_newline(close_brace_leading.trim_trailing_blanks(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_brace);

        PrintInfo::default_multi_lined()
    }
    fn leftmost_token(&self) -> TextRange {
        self.open_brace.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_brace.span()
    }
}

#[derive(Debug)]
pub enum ConfigBlockMember {
    Item(ConfigItem),
    TypeBuilder(TypeBuilderBlock),
    BlockAttribute(BlockAttribute),
}

impl Printable for ConfigBlockMember {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            ConfigBlockMember::Item(item) => item.print(shape, printer),
            ConfigBlockMember::TypeBuilder(block) => block.print(shape, printer),
            ConfigBlockMember::BlockAttribute(attr) => attr.print(shape, printer),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            ConfigBlockMember::Item(item) => item.leftmost_token(),
            ConfigBlockMember::TypeBuilder(block) => block.leftmost_token(),
            ConfigBlockMember::BlockAttribute(attr) => attr.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            ConfigBlockMember::Item(item) => item.rightmost_token(),
            ConfigBlockMember::TypeBuilder(block) => block.rightmost_token(),
            ConfigBlockMember::BlockAttribute(attr) => attr.rightmost_token(),
        }
    }
}

/// Corresponds to a [`SyntaxKind::CONFIG_ITEM`] node.
#[derive(Debug)]
pub struct ConfigItem {
    pub key: ConfigItemKey,
    pub colon: Option<t::Colon>,
    pub value: ConfigItemValue,
}

impl FromCST for ConfigItem {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::CONFIG_ITEM)?;

        let mut it = SyntaxNodeIter::new(&node);

        let key = it.expect_next("a CONFIG_ITEM key")?;
        let key = ConfigItemKey::from_cst(key)?;

        let colon = it
            .next_if_kind(SyntaxKind::COLON)
            .map(t::Colon::from_cst)
            .transpose()?;

        let value = it.expect_next("a config value")?;
        let value = ConfigItemValue::from_cst(value)?;

        it.expect_end()?;

        Ok(ConfigItem { key, colon, value })
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
        let colon_trailing = if let Some(colon) = &self.colon {
            let (_, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
            colon_trailing
        } else {
            &[][..]
        };
        printer.print_str(": ");
        printer.print_trivia_squished(colon_trailing);
        let value_leading = printer.trivia.get_leading_for_element(&self.value);
        printer.print_trivia_squished(value_leading);
        let remaining_width = printer.current_line_remaining_width();
        let value_shape = Shape {
            width: remaining_width.saturating_sub(const { ",".len() }),
            indent: shape.indent,
            first_line_offset: printer
                .config
                .line_width
                .saturating_sub(shape.indent + remaining_width),
        };
        multi_lined |= printer.print(&self.value, value_shape).multi_lined;
        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        self.key.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.value.rightmost_token()
    }
}

/// Any of the valid keys in a [`ConfigItem`].
///
/// See `Parser::parse_config_item` in [`baml_db::baml_compiler_parser`]
#[derive(Debug)]
pub enum ConfigItemKey {
    Word(t::Word),
    String(t::QuotedString),
    // parser allows raw strings as keys, but that's not a good idea
    // RawString(t::RawString),
    RetryPolicy(t::RetryPolicy),
    Enum(t::Enum),
    Class(t::Class),
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
            SyntaxKind::KW_ENUM => t::Enum::from_cst(elem).map(ConfigItemKey::Enum),
            SyntaxKind::KW_CLASS => t::Class::from_cst(elem).map(ConfigItemKey::Class),
            _ => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "WORD, STRING_LITERAL, KW_RETRY_POLICY, KW_ENUM, or KW_CLASS".into(),
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
            ConfigItemKey::Enum(enum_) => {
                printer.print_raw_token(enum_);
                PrintInfo::default_single_line()
            }
            ConfigItemKey::Class(class) => {
                printer.print_raw_token(class);
                PrintInfo::default_single_line()
            }
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            ConfigItemKey::Word(word) => word.span(),
            ConfigItemKey::String(string) => string.leftmost_token(),
            ConfigItemKey::RetryPolicy(retry_policy) => retry_policy.span(),
            ConfigItemKey::Enum(enum_) => enum_.span(),
            ConfigItemKey::Class(class) => class.span(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            ConfigItemKey::Word(word) => word.span(),
            ConfigItemKey::String(string) => string.rightmost_token(),
            ConfigItemKey::RetryPolicy(retry_policy) => retry_policy.span(),
            ConfigItemKey::Enum(enum_) => enum_.span(),
            ConfigItemKey::Class(class) => class.span(),
        }
    }
}

/// Any of the valid values in a [`ConfigItem`].
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
                let mut it = SyntaxNodeIter::new(&node);
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
    fn leftmost_token(&self) -> TextRange {
        match self {
            ConfigItemValue::Value(expr) => expr.leftmost_token(),
            ConfigItemValue::ConfigBlock(block) => block.leftmost_token(),
            ConfigItemValue::ConfigArray(array) => array.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            ConfigItemValue::Value(expr) => expr.rightmost_token(),
            ConfigItemValue::ConfigBlock(block) => block.rightmost_token(),
            ConfigItemValue::ConfigArray(array) => array.rightmost_token(),
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

        let mut it = SyntaxNodeIter::new(&node);

        let open_bracket = it.expect_parse()?;

        let mut elements = Vec::new();
        let close_bracket = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing(SyntaxKind::R_BRACKET, it.parent));
            };

            if elem.kind() == SyntaxKind::R_BRACKET {
                break t::RBracket::from_cst(elem)?;
            }

            let next = ConfigItemValue::from_cst(elem)?;
            let comma = it
                .next_if_kind(SyntaxKind::COMMA)
                .map(t::Comma::from_cst)
                .transpose()?;
            elements.push((next, comma));
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
        printer.print_trivia_all_trailing_for(self.open_bracket.span());
        printer.print_newline();

        for (elem, comma) in &self.elements {
            let (elem_leading, elem_trailing) = printer.trivia.get_for_element(elem);
            printer
                .print_trivia_with_newline(elem_leading.trim_leading_blanks(), inner_shape.indent);
            printer.print_spaces(inner_shape.indent);
            printer.print(elem, inner_shape.clone());
            if let Some(comma) = comma {
                printer.print_trivia_squished(elem_trailing);
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(comma.span());
                printer.print_trivia_squished(comma_leading);
                printer.print_raw_token(comma);
                printer.print_trivia_trailing(comma_trailing);
            } else {
                printer.print_str(",");
                printer.print_trivia_trailing(elem_trailing);
            }
            printer.print_newline();
        }

        printer.print_trivia_all_leading_with_newline_for(
            self.close_bracket.span(),
            inner_shape.indent,
        );
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_bracket);
        PrintInfo::default_multi_lined()
    }
}

impl ConfigArray {
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the config array on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.open_bracket);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_bracket.span());
        printer.try_print_trivia_single_line_squished(open_trailing)?;

        for (i, (elem, comma)) in self.elements.iter().enumerate() {
            if printer.output.len() > shape.width {
                return None;
            }
            let (el_leading, el_trailing) = printer.trivia.get_for_element(elem);
            printer.try_print_trivia_single_line_squished(el_leading)?;
            if printer
                .print(elem, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
            printer.try_print_trivia_single_line_squished(el_trailing)?;
            if i + 1 < self.elements.len() {
                // not the last element: will have comma
                if let Some(comma) = comma {
                    let (comma_leading, comma_trailing) =
                        printer.trivia.get_for_range_split(comma.span());
                    printer.print_trivia_squished(comma_leading);
                    printer.print_raw_token(comma);
                    printer.try_print_trivia_single_line_squished(comma_trailing)?;
                } else {
                    printer.print_str(",");
                }
                printer.print_str(" ");
            } else if let Some(comma) = comma {
                // Trailing comma is removed in single-line mode, but we still try the comments.
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(comma.span());
                printer.try_print_trivia_single_line_squished(comma_leading)?;
                printer.try_print_trivia_single_line_squished(comma_trailing)?;
            }
        }

        let (close_leading, _) = printer
            .trivia
            .get_for_range_split(self.close_bracket.span());
        printer.try_print_trivia_single_line_squished(close_leading)?;
        printer.print_raw_token(&self.close_bracket);

        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl Printable for ConfigArray {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.open_bracket.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_bracket.span()
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

        let mut it = SyntaxNodeIter::new(&node);

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
        let inner_indent = shape.indent + printer.config.indent_width;

        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print_raw_token(&self.open_brace);
        printer.print_trivia_all_trailing_for(self.open_brace.span());
        printer.print_newline();

        if let Some((first, rest)) = self.items.split_first() {
            let (first_leading, first_trailing) = printer.trivia.get_for_element(first);
            printer.print_trivia_with_newline(first_leading.trim_leading_blanks(), inner_indent);
            printer.print_spaces(inner_indent);
            let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
            printer.print(first, inner_shape);
            printer.print_trivia_trailing(first_trailing);
            printer.print_newline();

            for item in rest {
                printer.print_standalone_with_trivia(item, inner_indent);
                printer.print_newline();
            }
        }

        let (close_brace_leading, _) = printer.trivia.get_for_range_split(self.close_brace.span());
        printer.print_trivia_with_newline(close_brace_leading.trim_trailing_blanks(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_brace);
        PrintInfo::default_multi_lined()
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_brace.span()
    }
}

/// Any of the valid items in a [`TypeBuilderBlock`].
#[derive(Debug)]
pub enum TypeBuilderItem {
    /// Corresponds to a [`SyntaxKind::DYNAMIC_TYPE_DEF`] node that containins a class definition.
    DynamicClass(t::Dynamic, ClassDecl),
    /// Corresponds to a [`SyntaxKind::DYNAMIC_TYPE_DEF`] node that containins an enum definition.
    DynamicEnum(t::Dynamic, EnumDecl),
    Class(ClassDecl),
    Enum(EnumDecl),
    TypeAlias(TypeAliasDecl),
}

impl FromCST for TypeBuilderItem {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        match elem.kind() {
            SyntaxKind::DYNAMIC_TYPE_DEF => {
                let node = StrongAstError::assert_is_node(elem)?;
                let mut it = SyntaxNodeIter::new(&node);
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
            SyntaxKind::TYPE_ALIAS_DEF => {
                let alias = TypeAliasDecl::from_cst(elem)?;
                Ok(TypeBuilderItem::TypeAlias(alias))
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
            TypeBuilderItem::TypeAlias(alias) => printer.print(alias, shape),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            TypeBuilderItem::DynamicClass(dynamic, _) => dynamic.span(),
            TypeBuilderItem::DynamicEnum(dynamic, _) => dynamic.span(),
            TypeBuilderItem::Class(class) => class.leftmost_token(),
            TypeBuilderItem::Enum(enum_def) => enum_def.leftmost_token(),
            TypeBuilderItem::TypeAlias(alias) => alias.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            TypeBuilderItem::DynamicClass(_, class) => class.rightmost_token(),
            TypeBuilderItem::DynamicEnum(_, enum_def) => enum_def.rightmost_token(),
            TypeBuilderItem::Class(class) => class.rightmost_token(),
            TypeBuilderItem::Enum(enum_def) => enum_def.rightmost_token(),
            TypeBuilderItem::TypeAlias(alias) => alias.rightmost_token(),
        }
    }
}

/// Corresponds to a [`SyntaxKind::TEST_DEF`] node.
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

        let mut it = SyntaxNodeIter::new(&node);

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
        printer.print(&self.config_block, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.config_block.rightmost_token()
    }
}

/// Corresponds to a [`SyntaxKind::RETRY_POLICY_DEF`] node.
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

        let mut it = SyntaxNodeIter::new(&node);

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
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.config_block.rightmost_token()
    }
}

/// Corresponds to a [`SyntaxKind::TEMPLATE_STRING_DEF`] node.
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

        let mut it = SyntaxNodeIter::new(&node);

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
        multi_lined |= printer.print(&self.args, shape).multi_lined;
        printer.print_str(" ");
        multi_lined |= printer
            .print(&self.raw_string, Shape::unlimited_single_line())
            .multi_lined;
        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.raw_string.rightmost_token()
    }
}

/// Corresponds to a [`SyntaxKind::TYPE_ALIAS_DEF`] node.
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

        let mut it = SyntaxNodeIter::new(&node);

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
        let (_, eq_trailing) = printer.trivia.get_for_range_split(self.equals.span());
        let (ty_leading, ty_trailing) = printer.trivia.get_for_element(&self.type_expr);
        let mut ty_leading_len = printer.print_trivia_squished(eq_trailing);
        ty_leading_len += printer.print_trivia_squished(ty_leading);
        let new_offset = usize::from(self.keyword.span().len() + self.name.span().len())
            + const { "  = ".len() }
            + ty_leading_len;

        let info;
        if let Some(semicolon) = &self.semicolon {
            let (semicolon_leading, _) = printer.trivia.get_for_range_split(semicolon.span());
            let mut ty_trailing_len = ty_trailing.squished_len(printer.input);
            ty_trailing_len += semicolon_leading.squished_len(printer.input);
            let ty_shape = Shape {
                width: shape
                    .width
                    .saturating_sub(new_offset + ty_trailing_len + const { ";".len() }),
                indent: shape.indent,
                first_line_offset: shape.first_line_offset + new_offset,
            };
            info = printer.print(&self.type_expr, ty_shape);
            printer.print_trivia_squished(ty_trailing);
            printer.print_trivia_squished(semicolon_leading);
            printer.print_raw_token(semicolon);
        } else {
            let ty_shape = Shape {
                width: shape.width.saturating_sub(new_offset + const { ";".len() }),
                indent: shape.indent,
                first_line_offset: shape.first_line_offset + new_offset,
            };
            info = printer.print(&self.type_expr, ty_shape);
            // this is the last child so trivia is handled by parent
            printer.print_str(";");
        }

        info
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        if let Some(semicolon) = &self.semicolon {
            semicolon.span()
        } else {
            self.type_expr.rightmost_token()
        }
    }
}

/// Corresponds to a [`SyntaxKind::GENERATOR_DEF`] node.
#[derive(Debug)]
pub struct GeneratorDecl {
    pub keyword: t::Generator,
    pub name: t::Word,
    pub config: ConfigBlock,
}

impl FromCST for GeneratorDecl {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::GENERATOR_DEF)?;

        let mut it = SyntaxNodeIter::new(&node);

        let keyword = it.expect_parse()?;

        let name = it.expect_parse()?;

        let config = it.expect_parse()?;

        it.expect_end()?;

        Ok(GeneratorDecl {
            keyword,
            name,
            config,
        })
    }
}

impl KnownKind for GeneratorDecl {
    fn kind() -> SyntaxKind {
        SyntaxKind::GENERATOR_DEF
    }
}

impl Printable for GeneratorDecl {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print_raw_token(&self.name);
        printer.print_str(" ");
        printer.print(&self.config, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.config.rightmost_token()
    }
}
