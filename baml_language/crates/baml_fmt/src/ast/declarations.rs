use baml_db::baml_compiler_syntax::{SyntaxElement, SyntaxKind};
use rowan::TextRange;

use crate::{
    EmittableTrivia,
    ast::{
        Attribute, BlockAttribute, BlockExpr, Expression, FromCST, KnownKind, PathExpr,
        StrongAstError, SyntaxNodeIter, ThrowsClause, Token, Type, tokens as t,
    },
    printer::{PrintInfo, PrintMultiLine, Printable, Printer, Shape},
    trivia_classifier::TriviaSliceExt as _,
};

/// Any of the valid top-level declarations in a [`super::SourceFile`].
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum TopLevelDeclaration {
    Function(FunctionDecl),
    Class(ClassDecl),
    Enum(EnumDecl),
    Client(ClientDecl),
    TestExpr(TestExprDecl),
    TestSet(TestSetDecl),
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
            SyntaxKind::TEST_EXPR_DEF => {
                TopLevelDeclaration::TestExpr(TestExprDecl::from_cst(elem)?)
            }
            SyntaxKind::TESTSET_DEF => TopLevelDeclaration::TestSet(TestSetDecl::from_cst(elem)?),
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
            TopLevelDeclaration::TestExpr(test_expr_decl) => test_expr_decl.print(shape, printer),
            TopLevelDeclaration::TestSet(test_set_decl) => test_set_decl.print(shape, printer),
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
                let text = &printer.input[*range];
                printer.print_str(text.trim());
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
            TopLevelDeclaration::TestExpr(t) => t.leftmost_token(),
            TopLevelDeclaration::TestSet(t) => t.leftmost_token(),
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
            TopLevelDeclaration::TestExpr(t) => t.rightmost_token(),
            TopLevelDeclaration::TestSet(t) => t.rightmost_token(),
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
    pub generic_params: Option<super::GenericParamList>,
    pub params: FunctionParamList,
    pub arrow: super::FunctionArrow,
    pub return_type: Type,
    pub throws: Option<ThrowsClause>,
    pub body: FunctionDeclBody,
}
impl FromCST for FunctionDecl {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::FUNCTION_DEF)?;

        let mut it = SyntaxNodeIter::new(&node);

        let keyword = it.expect_parse()?;

        let name = it.expect_parse()?;

        let generic_params =
            if it.peek().map(SyntaxElement::kind) == Some(SyntaxKind::GENERIC_PARAM_LIST) {
                let elem = it.next().expect("peeked");
                Some(super::GenericParamList::from_cst(elem)?)
            } else {
                None
            };

        let params: FunctionParamList = it.expect_parse()?;

        let arrow = it.expect_parse()?;

        let return_type: Type = it.expect_parse()?;

        let throws = if it.peek().map(SyntaxElement::kind) == Some(SyntaxKind::THROWS_CLAUSE) {
            let elem = it.next().expect("peeked");
            Some(ThrowsClause::from_cst(elem)?)
        } else {
            None
        };

        let body = it.expect_node("of kind LLM_FUNCTION_BODY or EXPR_FUNCTION_BODY")?;
        let body = FunctionDeclBody::from_cst(SyntaxElement::Node(body))?;

        it.expect_end()?;

        Ok(FunctionDecl {
            keyword,
            name,
            generic_params,
            params,
            arrow,
            return_type,
            throws,
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
        if let Some(ref gp) = self.generic_params {
            printer.print(gp, shape.clone());
        }

        let mut param_printer = Printer::new_empty(printer.input, printer.config, printer.trivia);
        let param_info = param_printer.print(&self.params, Shape::unlimited_single_line());

        let mut return_type_printer =
            Printer::new_empty(printer.input, printer.config, printer.trivia);
        let return_type_info =
            return_type_printer.print(&self.return_type, Shape::unlimited_single_line());
        let (_, return_type_line_comment) =
            return_type_printer.print_trivia_all_trailing_for(self.return_type.rightmost_token());
        let mut throws_printer = Printer::new_empty(printer.input, printer.config, printer.trivia);
        let throws_info = self
            .throws
            .as_ref()
            .map(|throws| throws_printer.print(throws, Shape::unlimited_single_line()))
            .unwrap_or_else(PrintInfo::default_single_line);

        let single_line_size = printer.current_line_len()
            + param_printer.output.len()
            + const { " -> ".len() + " {".len() }
            + return_type_printer.output.len()
            + if self.throws.is_some() {
                (const { " ".len() }) + throws_printer.output.len()
            } else {
                0
            };
        if single_line_size <= printer.config.line_width
            && !param_info.multi_lined
            && !return_type_info.multi_lined
            && !throws_info.multi_lined
            && !return_type_line_comment
        {
            // It fits in single line!
            printer.append_from_printer(param_printer);
            printer.print_spaces(1);
            // Normalize the permissively accepted `=>` spelling to `->`.
            printer.print_str("->");
            self.arrow.print_separator_before(
                Some(self.return_type.leftmost_token()),
                shape.indent + printer.config.indent_width,
                printer,
            );
            printer.append_from_printer(return_type_printer);
            if self.throws.is_some() {
                printer.print_spaces(1);
                printer.append_from_printer(throws_printer);
            }
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
            // Normalize the permissively accepted `=>` spelling to `->`.
            printer.print_str("->");
            self.arrow.print_separator_before(
                Some(self.return_type.leftmost_token()),
                shape.indent + printer.config.indent_width,
                printer,
            );

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
            let throws_info = if let Some(ref throws) = self.throws {
                printer.print_str(" ");
                printer.print(throws, shape.clone())
            } else {
                PrintInfo::default_single_line()
            };

            if (return_info.multi_lined && self.return_type.multi_line_is_indented())
                || throws_info.multi_lined
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
    /// Type annotation with optional colon (colon is optional per BEP-019).
    pub ty: Option<(Option<t::Colon>, Type)>,
    pub default: Option<(t::Equals, Expression)>,
}

impl FromCST for FunctionParam {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::PARAMETER)?;

        let mut it = SyntaxNodeIter::new(&node);

        let name = it.expect_parse()?;

        let colon = it
            .next_if_kind(SyntaxKind::COLON)
            .map(t::Colon::from_cst)
            .transpose()?;
        let ty = if colon.is_some() {
            // Colon present - type is required
            let ty: Type = it.expect_parse()?;
            Some((colon, ty))
        } else if it.peek().map(SyntaxElement::kind) == Some(SyntaxKind::TYPE_EXPR) {
            // No colon but type present (BEP-019 optional colon)
            let elem = it.next().expect("peeked");
            Some((None, Type::from_cst(elem)?))
        } else {
            // No type annotation (e.g. `self`)
            None
        };

        let default = if let Some(equals) = it.next_if_kind(SyntaxKind::EQUALS) {
            let equals = t::Equals::from_cst(equals)?;
            let expr_elem = it
                .next()
                .ok_or_else(|| StrongAstError::missing_desc("default expression", it.parent))?;
            let expr = Expression::from_cst(expr_elem)?;
            Some((equals, expr))
        } else {
            None
        };

        it.expect_end()?;

        Ok(FunctionParam { name, ty, default })
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
        let mut info = if let Some((colon, ty)) = &self.ty {
            let mut trivia_len = 0;
            // Colon is optional per BEP-019; synthesize if absent
            if let Some(colon) = colon {
                let (_, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
                printer.print_str(": ");
                trivia_len += printer.print_trivia_squished(colon_trailing);
            } else {
                printer.print_str(": ");
            }
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
        };

        if let Some((equals, default)) = &self.default {
            let prev_token = self
                .ty
                .as_ref()
                .map_or_else(|| self.name.span(), |(_, ty)| ty.rightmost_token());
            let (_, prev_trailing) = printer.trivia.get_for_range_split(prev_token);
            let (equals_leading, equals_trailing) =
                printer.trivia.get_for_range_split(equals.span());
            printer.print_trivia_squished(prev_trailing);
            printer.print_trivia_squished(equals_leading);
            printer.print_str(" = ");
            printer.print_trivia_squished(equals_trailing);
            let leading = printer.trivia.get_leading_for_element(default);
            printer.print_trivia_squished(leading);
            info = printer.print(default, shape);
        }

        info
    }
    fn leftmost_token(&self) -> TextRange {
        self.name.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.default.as_ref().map_or_else(
            || {
                self.ty
                    .as_ref()
                    .map_or(self.name.span(), |(_, ty)| ty.rightmost_token())
            },
            |(_, default)| default.rightmost_token(),
        )
    }
}

/// Any of the valid function bodies in a [`FunctionDecl`].
#[derive(Debug)]
pub enum FunctionDeclBody {
    // Boxed: the LLM body (client/tools/prompt fields) dwarfs `BlockExpr`
    // (clippy::large_enum_variant).
    Llm(Box<LlmFunctionBody>),
    Block(BlockExpr),
}
impl FromCST for FunctionDeclBody {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        match node.kind() {
            SyntaxKind::LLM_FUNCTION_BODY => Ok(FunctionDeclBody::Llm(Box::new(
                LlmFunctionBody::from_cst(SyntaxElement::Node(node))?,
            ))),
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
    /// Fields may appear in any order in the input; printing canonicalizes to
    /// client, tools, prompt.
    pub client: ClientField,
    /// Optional `tools: [a, b]` list (BEP spec mode).
    pub tools: Option<ToolsField>,
    pub prompt: PromptField,
    pub close_brace: t::RBrace,
}
impl FromCST for LlmFunctionBody {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        // A duplicate LLM-body field is a hard error, not an overwrite:
        // silently printing only the survivor would delete the other field
        // from the user's source. An errored declaration is left unformatted.
        fn fill<T>(
            slot: &mut Option<T>,
            value: T,
            kind: SyntaxKind,
            parent_range: TextRange,
        ) -> Result<(), StrongAstError> {
            if slot.is_some() {
                return Err(StrongAstError::UnexpectedKindDesc {
                    expected_desc: "at most one field of each kind in an LLM function body".into(),
                    found: kind,
                    at: parent_range,
                });
            }
            *slot = Some(value);
            Ok(())
        }

        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::LLM_FUNCTION_BODY)?;

        let mut it = SyntaxNodeIter::new(&node);

        let open_brace = it.expect_parse()?;

        // Fields appear in any order; collect until the close brace.
        let mut client: Option<ClientField> = None;
        let mut tools: Option<ToolsField> = None;
        let mut prompt: Option<PromptField> = None;
        loop {
            if let Some(n) = it.next_if_kind(SyntaxKind::CLIENT_FIELD) {
                fill(
                    &mut client,
                    ClientField::from_cst(n)?,
                    SyntaxKind::CLIENT_FIELD,
                    node.text_range(),
                )?;
            } else if let Some(n) = it.next_if_kind(SyntaxKind::TOOLS_FIELD) {
                fill(
                    &mut tools,
                    ToolsField::from_cst(n)?,
                    SyntaxKind::TOOLS_FIELD,
                    node.text_range(),
                )?;
            } else if let Some(n) = it.next_if_kind(SyntaxKind::PROMPT_FIELD) {
                fill(
                    &mut prompt,
                    PromptField::from_cst(n)?,
                    SyntaxKind::PROMPT_FIELD,
                    node.text_range(),
                )?;
            } else {
                break;
            }
        }
        let client = client.ok_or(StrongAstError::MissingExpectedElement {
            expected: SyntaxKind::CLIENT_FIELD,
            parent: node.text_range(),
        })?;
        let prompt = prompt.ok_or(StrongAstError::MissingExpectedElement {
            expected: SyntaxKind::PROMPT_FIELD,
            parent: node.text_range(),
        })?;

        let close_brace = it.expect_parse()?;

        it.expect_end()?;

        Ok(LlmFunctionBody {
            open_brace,
            client,
            tools,
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

        if let Some(tools) = &self.tools {
            printer.print_standalone_with_trivia(tools, inner_indent);
            printer.print_newline();
        }

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
    pub colon: t::Colon,
    pub name: ClientName,
}

impl FromCST for ClientField {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::CLIENT_FIELD)?;

        let mut it = SyntaxNodeIter::new(&node);

        let keyword = it.expect_parse()?;

        let colon = it.expect_parse()?;

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
                ClientName::Path(PathExpr {
                    first,
                    rest,
                    generic_args: None,
                })
            }
            SyntaxKind::PATH_EXPR => ClientName::Path(PathExpr::from_cst(name)?),
            // Any other node is an ai.Client expression (a constructor call,
            // a wrapper, ...) — print through the expression machinery.
            _ => ClientName::Expr(Box::new(Expression::from_cst(name)?)),
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
        let (_, keyword_trailing) = printer.trivia.get_for_range_split(self.keyword.span());
        printer.print_trivia_squished(keyword_trailing);
        let (colon_leading, colon_trailing) = printer.trivia.get_for_range_split(self.colon.span());
        printer.print_trivia_squished(colon_leading);
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
    /// An arbitrary ai.Client expression (`client: openai.ResponsesClient.new(...)`).
    Expr(Box<Expression>),
}

impl Printable for ClientName {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            ClientName::Path(path) => printer.print(path, shape),
            ClientName::String(string) => printer.print(string, shape),
            ClientName::Expr(expr) => printer.print(expr.as_ref(), shape),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            ClientName::Path(path) => path.leftmost_token(),
            ClientName::String(string) => string.leftmost_token(),
            ClientName::Expr(expr) => expr.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            ClientName::Path(path) => path.rightmost_token(),
            ClientName::String(string) => string.rightmost_token(),
            ClientName::Expr(expr) => expr.rightmost_token(),
        }
    }
}

/// Corresponds to a [`SyntaxKind::PROMPT_FIELD`] node.
#[derive(Debug)]
pub struct PromptField {
    pub prompt: t::Word,
    pub colon: t::Colon,
    pub string: StringLiteralValue,
}

impl FromCST for PromptField {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::PROMPT_FIELD)?;

        let mut it = SyntaxNodeIter::new(&node);

        // It's a word, but we should never be in a `PROMPT_FIELD` context if it's not a prompt
        let prompt = it.expect_parse()?;

        let colon = it.expect_parse()?;

        let string = StringLiteralValue::from_cst(it.expect_next("a prompt string")?)?;

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
        let (_, prompt_trailing) = printer.trivia.get_for_range_split(self.prompt.span());
        printer.print_trivia_squished(prompt_trailing);
        let (colon_leading, colon_trailing) = printer.trivia.get_for_range_split(self.colon.span());
        printer.print_trivia_squished(colon_leading);
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

/// Corresponds to a [`SyntaxKind::TOOLS_FIELD`] node: `tools: [a, b]` in an
/// LLM function body (BEP spec mode). The value is an arbitrary expression
/// producing the tool list.
#[derive(Debug)]
pub struct ToolsField {
    pub keyword: t::Word,
    pub colon: t::Colon,
    pub value: Expression,
}

impl FromCST for ToolsField {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::TOOLS_FIELD)?;

        let mut it = SyntaxNodeIter::new(&node);

        // It's a word; we are only in a TOOLS_FIELD context if it is `tools`.
        let keyword = it.expect_parse()?;

        let colon = it.expect_parse()?;

        let value = Expression::from_cst(it.expect_next("a tools expression")?)?;

        it.expect_end()?;

        Ok(ToolsField {
            keyword,
            colon,
            value,
        })
    }
}

impl KnownKind for ToolsField {
    fn kind() -> SyntaxKind {
        SyntaxKind::TOOLS_FIELD
    }
}

impl Printable for ToolsField {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        let (_, keyword_trailing) = printer.trivia.get_for_range_split(self.keyword.span());
        printer.print_trivia_squished(keyword_trailing);
        let (colon_leading, colon_trailing) = printer.trivia.get_for_range_split(self.colon.span());
        printer.print_trivia_squished(colon_leading);
        printer.print_str(": ");
        printer.print_trivia_squished(colon_trailing);
        let value_leading = printer.trivia.get_leading_for_element(&self.value);
        printer.print_trivia_squished(value_leading);
        printer.print(&self.value, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.value.rightmost_token()
    }
}

/// A string-literal value as it appears in a declarative slot such as a
/// [`PromptField`] or a [`TemplateStringDecl`]: a raw `#"..."#`, a quoted
/// `"..."`, or a backtick `` `...` `` literal. All three parse equally in these
/// positions, so the formatter accepts and re-emits any of them.
#[derive(Debug)]
pub enum StringLiteralValue {
    RawString(t::RawString),
    String(t::QuotedString),
    Backtick(t::BacktickString),
}

impl FromCST for StringLiteralValue {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        match elem.kind() {
            SyntaxKind::RAW_STRING_LITERAL => {
                Ok(StringLiteralValue::RawString(t::RawString::from_cst(elem)?))
            }
            SyntaxKind::STRING_LITERAL => {
                Ok(StringLiteralValue::String(t::QuotedString::from_cst(elem)?))
            }
            SyntaxKind::BACKTICK_STRING_LITERAL => Ok(StringLiteralValue::Backtick(
                t::BacktickString::from_cst(elem)?,
            )),
            found => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "STRING_LITERAL, RAW_STRING_LITERAL, or BACKTICK_STRING_LITERAL"
                    .into(),
                found,
                at: elem.text_range(),
            }),
        }
    }
}

impl Printable for StringLiteralValue {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            StringLiteralValue::RawString(raw_string) => printer.print(raw_string, shape),
            StringLiteralValue::String(string) => printer.print(string, shape),
            StringLiteralValue::Backtick(backtick) => printer.print(backtick, shape),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            StringLiteralValue::RawString(raw_string) => raw_string.leftmost_token(),
            StringLiteralValue::String(string) => string.leftmost_token(),
            StringLiteralValue::Backtick(backtick) => backtick.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            StringLiteralValue::RawString(raw_string) => raw_string.rightmost_token(),
            StringLiteralValue::String(string) => string.rightmost_token(),
            StringLiteralValue::Backtick(backtick) => backtick.rightmost_token(),
        }
    }
}

/// Corresponds to a [`SyntaxKind::CLASS_DEF`] node.
#[derive(Debug)]
pub struct ClassDecl {
    pub keyword: t::Class,
    pub name: t::Word,
    pub generic_params: Option<super::GenericParamList>,
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

        let generic_params =
            if it.peek().map(SyntaxElement::kind) == Some(SyntaxKind::GENERIC_PARAM_LIST) {
                let elem = it.next().expect("peeked");
                Some(super::GenericParamList::from_cst(elem)?)
            } else {
                None
            };

        let open_brace = it.expect_parse()?;

        // collect class items (fields, functions, block attributes)
        let mut items = Vec::new();

        let close_brace = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing(SyntaxKind::R_BRACE, it.parent));
            };
            match elem.kind() {
                SyntaxKind::FIELD => {
                    let field = ClassField::from_cst(elem)?;
                    let delimiter = if let Some(comma_elem) = it.next_if_kind(SyntaxKind::COMMA) {
                        Some(ClassFieldDelimiter::Comma(t::Comma::from_cst(comma_elem)?))
                    } else if let Some(semi_elem) = it.next_if_kind(SyntaxKind::SEMICOLON) {
                        Some(ClassFieldDelimiter::Semicolon(t::Semicolon::from_cst(
                            semi_elem,
                        )?))
                    } else {
                        None
                    };
                    items.push(ClassItem::Field(field, delimiter));
                }
                SyntaxKind::FUNCTION_DEF => {
                    items.push(ClassItem::Function(FunctionDecl::from_cst(elem)?));
                }
                SyntaxKind::IMPLEMENTS_BLOCK => {
                    items.push(ClassItem::Implements(ImplementsBlock::from_cst(elem)?));
                }
                SyntaxKind::BLOCK_ATTRIBUTE => {
                    items.push(ClassItem::BlockAttribute(BlockAttribute::from_cst(elem)?));
                }
                SyntaxKind::COMMA | SyntaxKind::SEMICOLON => {
                    // Stray delimiter not following a field - skip silently
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
            generic_params,
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
        if let Some(ref gp) = self.generic_params {
            printer.print(gp, shape.clone());
        }
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

/// Delimiter after a class field (comma or semicolon).
/// The formatter normalizes to comma, but we preserve the original for trivia.
#[derive(Debug)]
pub enum ClassFieldDelimiter {
    Comma(t::Comma),
    Semicolon(t::Semicolon),
}

/// Corresponds to a [`SyntaxKind::IMPLEMENTS_TARGET`] node.
#[derive(Debug)]
pub struct ImplementsTarget {
    pub ty: Type,
}

impl FromCST for ImplementsTarget {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::IMPLEMENTS_TARGET)?;

        let mut it = SyntaxNodeIter::new(&node);
        let ty = it.expect_parse()?;
        it.expect_end()?;

        Ok(ImplementsTarget { ty })
    }
}

impl KnownKind for ImplementsTarget {
    fn kind() -> SyntaxKind {
        SyntaxKind::IMPLEMENTS_TARGET
    }
}

impl Printable for ImplementsTarget {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        self.ty.print(shape, printer)
    }

    fn leftmost_token(&self) -> TextRange {
        self.ty.leftmost_token()
    }

    fn rightmost_token(&self) -> TextRange {
        self.ty.rightmost_token()
    }
}

/// BEP-057 associated type declaration or implementation witness.
#[derive(Debug)]
pub struct AssociatedTypeDecl {
    pub keyword: t::TypeKw,
    pub name: t::Word,
    pub bound: Option<(t::Extends, Type)>,
    pub default: Option<(t::Equals, Type)>,
}

impl FromCST for AssociatedTypeDecl {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::ASSOCIATED_TYPE_DECL)?;

        let mut it = SyntaxNodeIter::new(&node);
        let keyword = it.expect_parse()?;
        let name = it.expect_parse()?;
        let mut bound = None;
        let mut default = None;

        while let Some(elem) = it.next() {
            match elem.kind() {
                SyntaxKind::KW_EXTENDS => {
                    let extends = t::Extends::from_cst(elem)?;
                    let ty = it.expect_parse()?;
                    bound = Some((extends, ty));
                }
                SyntaxKind::EQUALS => {
                    let equals = t::Equals::from_cst(elem)?;
                    let ty = it.expect_parse()?;
                    default = Some((equals, ty));
                }
                _ => {
                    return Err(StrongAstError::UnexpectedAdditionalElement {
                        parent: it.parent,
                        at: elem.text_range(),
                    });
                }
            }
        }

        Ok(AssociatedTypeDecl {
            keyword,
            name,
            bound,
            default,
        })
    }
}

impl Printable for AssociatedTypeDecl {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print_raw_token(&self.name);
        if let Some((extends, ty)) = &self.bound {
            let (_, extends_trailing) = printer.trivia.get_for_range_split(extends.span());
            printer.print_str(" extends ");
            printer.print_trivia_squished(extends_trailing);
            let leading = printer.trivia.get_leading_for_element(ty);
            printer.print_trivia_squished(leading);
            multi_lined |= ty.print(shape.clone(), printer).multi_lined;
        }
        if let Some((equals, ty)) = &self.default {
            let (_, equals_trailing) = printer.trivia.get_for_range_split(equals.span());
            printer.print_str(" = ");
            printer.print_trivia_squished(equals_trailing);
            let leading = printer.trivia.get_leading_for_element(ty);
            printer.print_trivia_squished(leading);
            multi_lined |= ty.print(shape, printer).multi_lined;
        }
        PrintInfo { multi_lined }
    }

    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.default
            .as_ref()
            .map(|(_, ty)| ty.rightmost_token())
            .or_else(|| self.bound.as_ref().map(|(_, ty)| ty.rightmost_token()))
            .unwrap_or_else(|| self.name.span())
    }
}

/// Corresponds to a [`SyntaxKind::INTERFACE_FIELD_LINK`] node.
#[derive(Debug)]
pub struct InterfaceFieldLink {
    pub interface_field: t::Word,
    pub as_token: t::As,
    pub class_field: t::Word,
}

impl FromCST for InterfaceFieldLink {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::INTERFACE_FIELD_LINK)?;

        let mut it = SyntaxNodeIter::new(&node);
        let interface_field = it.expect_parse()?;
        let as_token = it.expect_parse()?;
        let class_field = it.expect_parse()?;
        it.expect_end()?;

        Ok(InterfaceFieldLink {
            interface_field,
            as_token,
            class_field,
        })
    }
}

impl Printable for InterfaceFieldLink {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.interface_field);
        printer.print_str(" ");
        printer.print_raw_token(&self.as_token);
        printer.print_str(" ");
        printer.print_raw_token(&self.class_field);
        PrintInfo::default_single_line()
    }

    fn leftmost_token(&self) -> TextRange {
        self.interface_field.span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.class_field.span()
    }
}

/// Any item accepted inside a class `implements` block.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ImplementsItem {
    AssociatedType(AssociatedTypeDecl, Option<ClassFieldDelimiter>),
    FieldLink(InterfaceFieldLink, Option<ClassFieldDelimiter>),
    Field(ClassField, Option<ClassFieldDelimiter>),
    Function(FunctionDecl),
}

impl ImplementsItem {
    fn delimiter_rightmost(
        delimiter: Option<&ClassFieldDelimiter>,
        fallback: impl FnOnce() -> TextRange,
    ) -> TextRange {
        match delimiter {
            Some(ClassFieldDelimiter::Comma(comma)) => comma.span(),
            Some(ClassFieldDelimiter::Semicolon(semi)) => semi.span(),
            None => fallback(),
        }
    }
}

impl Printable for ImplementsItem {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            ImplementsItem::AssociatedType(decl, _) => decl.print(shape, printer),
            ImplementsItem::FieldLink(link, _) => link.print(shape, printer),
            ImplementsItem::Field(field, delimiter) => {
                let info = field.print(shape, printer);
                match delimiter {
                    Some(ClassFieldDelimiter::Comma(comma)) => printer.print_raw_token(comma),
                    Some(ClassFieldDelimiter::Semicolon(_)) | None => {}
                }
                info
            }
            ImplementsItem::Function(function) => function.print(shape, printer),
        }
    }

    fn leftmost_token(&self) -> TextRange {
        match self {
            ImplementsItem::AssociatedType(decl, _) => decl.leftmost_token(),
            ImplementsItem::FieldLink(link, _) => link.leftmost_token(),
            ImplementsItem::Field(field, _) => field.leftmost_token(),
            ImplementsItem::Function(function) => function.leftmost_token(),
        }
    }

    fn rightmost_token(&self) -> TextRange {
        match self {
            ImplementsItem::AssociatedType(decl, delimiter) => {
                Self::delimiter_rightmost(delimiter.as_ref(), || decl.rightmost_token())
            }
            ImplementsItem::FieldLink(link, delimiter) => {
                Self::delimiter_rightmost(delimiter.as_ref(), || link.rightmost_token())
            }
            ImplementsItem::Field(field, delimiter) => {
                Self::delimiter_rightmost(delimiter.as_ref(), || field.rightmost_token())
            }
            ImplementsItem::Function(function) => function.rightmost_token(),
        }
    }
}

/// Corresponds to a [`SyntaxKind::IMPLEMENTS_BLOCK`] node.
#[derive(Debug)]
pub struct ImplementsBlock {
    pub keyword_span: TextRange,
    pub target: ImplementsTarget,
    pub open_brace: t::LBrace,
    pub items: Vec<ImplementsItem>,
    pub close_brace: t::RBrace,
}

impl FromCST for ImplementsBlock {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::IMPLEMENTS_BLOCK)?;

        let mut it = SyntaxNodeIter::new(&node);
        let keyword = it.expect_next("implements or implement")?;
        match keyword.kind() {
            SyntaxKind::KW_IMPLEMENTS | SyntaxKind::KW_IMPLEMENT => {}
            found => {
                return Err(StrongAstError::UnexpectedKindDesc {
                    expected_desc: "implements or implement".into(),
                    found,
                    at: keyword.text_range(),
                });
            }
        }
        let target = it.expect_parse()?;
        let open_brace = it.expect_parse()?;
        let mut items = Vec::new();

        let close_brace = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing(SyntaxKind::R_BRACE, it.parent));
            };
            match elem.kind() {
                SyntaxKind::ASSOCIATED_TYPE_DECL => {
                    let decl = AssociatedTypeDecl::from_cst(elem)?;
                    let delimiter = if let Some(comma_elem) = it.next_if_kind(SyntaxKind::COMMA) {
                        Some(ClassFieldDelimiter::Comma(t::Comma::from_cst(comma_elem)?))
                    } else if let Some(semi_elem) = it.next_if_kind(SyntaxKind::SEMICOLON) {
                        Some(ClassFieldDelimiter::Semicolon(t::Semicolon::from_cst(
                            semi_elem,
                        )?))
                    } else {
                        None
                    };
                    items.push(ImplementsItem::AssociatedType(decl, delimiter));
                }
                SyntaxKind::INTERFACE_FIELD_LINK => {
                    let link = InterfaceFieldLink::from_cst(elem)?;
                    let delimiter = if let Some(comma_elem) = it.next_if_kind(SyntaxKind::COMMA) {
                        Some(ClassFieldDelimiter::Comma(t::Comma::from_cst(comma_elem)?))
                    } else if let Some(semi_elem) = it.next_if_kind(SyntaxKind::SEMICOLON) {
                        Some(ClassFieldDelimiter::Semicolon(t::Semicolon::from_cst(
                            semi_elem,
                        )?))
                    } else {
                        None
                    };
                    items.push(ImplementsItem::FieldLink(link, delimiter));
                }
                SyntaxKind::FIELD => {
                    let field = ClassField::from_cst(elem)?;
                    let delimiter = if let Some(comma_elem) = it.next_if_kind(SyntaxKind::COMMA) {
                        Some(ClassFieldDelimiter::Comma(t::Comma::from_cst(comma_elem)?))
                    } else if let Some(semi_elem) = it.next_if_kind(SyntaxKind::SEMICOLON) {
                        Some(ClassFieldDelimiter::Semicolon(t::Semicolon::from_cst(
                            semi_elem,
                        )?))
                    } else {
                        None
                    };
                    items.push(ImplementsItem::Field(field, delimiter));
                }
                SyntaxKind::FUNCTION_DEF => {
                    items.push(ImplementsItem::Function(FunctionDecl::from_cst(elem)?));
                }
                SyntaxKind::COMMA | SyntaxKind::SEMICOLON => {}
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

        Ok(ImplementsBlock {
            keyword_span: keyword.text_range(),
            target,
            open_brace,
            items,
            close_brace,
        })
    }
}

impl Printable for ImplementsBlock {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_str("implements");
        let (_, keyword_trailing) = printer.trivia.get_for_range_split(self.keyword_span);
        let trivia_len = printer.print_trivia_squished(keyword_trailing);
        if trivia_len == 0 {
            printer.print_str(" ");
        }
        let target_leading = printer.trivia.get_leading_for_element(&self.target);
        printer.print_trivia_squished(target_leading);
        printer.print(&self.target, shape.clone());

        if self.items.is_empty() {
            printer.print_str(" ");
            printer.print_raw_token(&self.open_brace);
            printer.print_raw_token(&self.close_brace);
            return PrintInfo::default_single_line();
        }

        let inner_indent = shape.indent + printer.config.indent_width;
        printer.print_str(" ");
        printer.print_raw_token(&self.open_brace);
        printer.print_trivia_all_trailing_for(self.open_brace.span());
        printer.print_newline();

        if let Some((first, rest)) = self.items.split_first() {
            let (first_leading, first_trailing) = printer.trivia.get_for_element(first);
            printer.print_trivia_with_newline(first_leading.trim_leading_blanks(), inner_indent);
            printer.print_spaces(inner_indent);
            let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
            first.print(inner_shape, printer);
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
        self.keyword_span
    }

    fn rightmost_token(&self) -> TextRange {
        self.close_brace.span()
    }
}

/// Any of the valid items in a [`ClassDecl`].
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ClassItem {
    Field(ClassField, Option<ClassFieldDelimiter>),
    Function(FunctionDecl),
    Implements(ImplementsBlock),
    BlockAttribute(BlockAttribute),
    Unknown(TextRange),
}

impl FromCST for ClassItem {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let item = match elem.kind() {
            SyntaxKind::FIELD => ClassItem::Field(ClassField::from_cst(elem)?, None),
            SyntaxKind::FUNCTION_DEF => ClassItem::Function(FunctionDecl::from_cst(elem)?),
            SyntaxKind::IMPLEMENTS_BLOCK => ClassItem::Implements(ImplementsBlock::from_cst(elem)?),
            SyntaxKind::BLOCK_ATTRIBUTE => {
                ClassItem::BlockAttribute(BlockAttribute::from_cst(elem)?)
            }
            found => {
                return Err(StrongAstError::UnexpectedKindDesc {
                    expected_desc: "FIELD, FUNCTION_DEF, IMPLEMENTS_BLOCK, or BLOCK_ATTRIBUTE"
                        .into(),
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
            ClassItem::Field(field, delimiter) => {
                let info = field.print(shape, printer);
                // Always print comma, but preserve trivia from original delimiter
                match delimiter {
                    Some(ClassFieldDelimiter::Comma(comma)) => {
                        printer.print_raw_token(comma);
                    }
                    Some(ClassFieldDelimiter::Semicolon(_)) => {
                        // Normalize to comma; parent handles trailing trivia via rightmost_token()
                        printer.print_str(",");
                    }
                    None => {
                        printer.print_str(",");
                    }
                }
                info
            }
            ClassItem::Function(function) => function.print(shape, printer),
            ClassItem::Implements(block) => block.print(shape, printer),
            ClassItem::BlockAttribute(attr) => attr.print(shape, printer),
            ClassItem::Unknown(range) => {
                printer.print_input_range(*range);
                PrintInfo::default_multi_lined()
            }
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            ClassItem::Field(field, _) => field.leftmost_token(),
            ClassItem::Function(function) => function.leftmost_token(),
            ClassItem::Implements(block) => block.leftmost_token(),
            ClassItem::BlockAttribute(attr) => attr.leftmost_token(),
            ClassItem::Unknown(range) => *range,
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            ClassItem::Field(field, delimiter) => match delimiter {
                Some(ClassFieldDelimiter::Comma(comma)) => comma.span(),
                Some(ClassFieldDelimiter::Semicolon(semi)) => semi.span(),
                None => field.rightmost_token(),
            },
            ClassItem::Function(function) => function.rightmost_token(),
            ClassItem::Implements(block) => block.rightmost_token(),
            ClassItem::BlockAttribute(attr) => attr.rightmost_token(),
            ClassItem::Unknown(range) => *range,
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

                    let delimiter = match it.peek().map(SyntaxElement::kind) {
                        Some(SyntaxKind::COMMA) => Some(EnumVariantDelimiter::Comma(
                            t::Comma::from_cst(it.next().expect("peeked"))?,
                        )),
                        Some(SyntaxKind::SEMICOLON) => Some(EnumVariantDelimiter::Semicolon(
                            t::Semicolon::from_cst(it.next().expect("peeked"))?,
                        )),
                        _ => None,
                    };

                    items.push(EnumItem::Variant(variant, delimiter));
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
    Variant(EnumVariant, Option<EnumVariantDelimiter>),
    BlockAttribute(BlockAttribute),
}

#[derive(Debug)]
pub enum EnumVariantDelimiter {
    Comma(t::Comma),
    Semicolon(t::Semicolon),
}

impl EnumVariantDelimiter {
    fn span(&self) -> TextRange {
        match self {
            Self::Comma(comma) => comma.span(),
            Self::Semicolon(semicolon) => semicolon.span(),
        }
    }
}

impl Printable for EnumItem {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            EnumItem::Variant(variant, delimiter) => {
                let info = variant.print(shape, printer);
                if let Some(delimiter) = delimiter {
                    let (leading, _) = printer.trivia.get_for_range_split(delimiter.span());
                    printer.print_trivia_squished(leading);
                }
                printer.print_str(",");
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
            EnumItem::Variant(variant, delimiter) => {
                if let Some(delimiter) = delimiter {
                    delimiter.span()
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
            let elem = it.expect_next("CONFIG_ITEM, BLOCK_ATTRIBUTE, or R_BRACE")?;

            let item = match elem.kind() {
                SyntaxKind::R_BRACE => break t::RBrace::from_cst(elem)?,
                SyntaxKind::CONFIG_ITEM => ConfigBlockMember::Item(ConfigItem::from_cst(elem)?),
                SyntaxKind::BLOCK_ATTRIBUTE => {
                    ConfigBlockMember::BlockAttribute(BlockAttribute::from_cst(elem)?)
                }
                _ => {
                    return Err(StrongAstError::UnexpectedKindDesc {
                        expected_desc: "CONFIG_ITEM, BLOCK_ATTRIBUTE, or R_BRACE".into(),
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
                ConfigBlockMember::Item(_) => None,
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
    BlockAttribute(BlockAttribute),
}

impl Printable for ConfigBlockMember {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            ConfigBlockMember::Item(item) => item.print(shape, printer),
            ConfigBlockMember::BlockAttribute(attr) => attr.print(shape, printer),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            ConfigBlockMember::Item(item) => item.leftmost_token(),
            ConfigBlockMember::BlockAttribute(attr) => attr.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            ConfigBlockMember::Item(item) => item.rightmost_token(),
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
            SyntaxKind::WORD | SyntaxKind::KW_CLIENT => {
                t::Word::from_cst(elem).map(ConfigItemKey::Word)
            }
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

/// The `with <expr>` clause on test/testset declarations.
#[derive(Debug)]
pub struct WithClause {
    pub keyword: t::With,
    pub expr: Expression,
}

/// Corresponds to a [`SyntaxKind::TEST_EXPR_DEF`] node.
#[derive(Debug)]
pub struct TestExprDecl {
    pub keyword: t::Test,
    /// Test name — any expression that evaluates to a string. The parser
    /// accepts string literals, raw strings, identifiers, concatenations,
    /// arithmetic, etc.; type-checking enforces the string requirement.
    pub name: Expression,
    pub with_clause: Option<WithClause>,
    pub body: BlockExpr,
}

impl FromCST for TestExprDecl {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::TEST_EXPR_DEF)?;

        let mut it = SyntaxNodeIter::new(&node);

        // keyword: "test"
        let keyword = it.expect_parse()?;

        // name — any expression
        let name_elem = it.expect_next("a test name expression")?;
        let name = Expression::from_cst(name_elem)?;

        // optional `with` clause
        let with_clause = if let Some(with_kw_elem) = it.next_if_kind(SyntaxKind::KW_WITH) {
            let with_kw = t::With::from_cst(with_kw_elem)?;
            let expr_elem = it.expect_next("a runner expression")?;
            let expr = Expression::from_cst(expr_elem)?;
            Some(WithClause {
                keyword: with_kw,
                expr,
            })
        } else {
            None
        };

        // block body
        let body: BlockExpr = it.expect_parse()?;

        it.expect_end()?;

        Ok(TestExprDecl {
            keyword,
            name,
            with_clause,
            body,
        })
    }
}

impl KnownKind for TestExprDecl {
    fn kind() -> SyntaxKind {
        SyntaxKind::TEST_EXPR_DEF
    }
}

impl Printable for TestExprDecl {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print(&self.name, shape.clone());
        if let Some(wc) = &self.with_clause {
            printer.print_str(" ");
            printer.print_raw_token(&wc.keyword);
            printer.print_str(" ");
            printer.print(&wc.expr, shape.clone());
        }
        printer.print_str(" ");
        printer.print(&self.body, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.body.rightmost_token()
    }
}

/// Corresponds to a [`SyntaxKind::TESTSET_DEF`] node.
#[derive(Debug)]
pub struct TestSetDecl {
    pub keyword: t::TestSet,
    /// Testset name — any expression (string literal, raw string, identifier,
    /// concatenation, etc.); type-checking enforces the string requirement.
    pub name: Expression,
    pub with_clause: Option<WithClause>,
    pub body: BlockExpr,
}

impl FromCST for TestSetDecl {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::TESTSET_DEF)?;

        let mut it = SyntaxNodeIter::new(&node);

        // keyword: "testset"
        let keyword = it.expect_parse()?;

        // name — any expression
        let name_elem = it.expect_next("a testset name expression")?;
        let name = Expression::from_cst(name_elem)?;

        // optional `with` clause
        let with_clause = if let Some(with_kw_elem) = it.next_if_kind(SyntaxKind::KW_WITH) {
            let with_kw = t::With::from_cst(with_kw_elem)?;
            let expr_elem = it.expect_next("a runner expression")?;
            let expr = Expression::from_cst(expr_elem)?;
            Some(WithClause {
                keyword: with_kw,
                expr,
            })
        } else {
            None
        };

        // block body
        let body: BlockExpr = it.expect_parse()?;

        it.expect_end()?;

        Ok(TestSetDecl {
            keyword,
            name,
            with_clause,
            body,
        })
    }
}

impl KnownKind for TestSetDecl {
    fn kind() -> SyntaxKind {
        SyntaxKind::TESTSET_DEF
    }
}

impl Printable for TestSetDecl {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print(&self.name, shape.clone());
        if let Some(wc) = &self.with_clause {
            printer.print_str(" ");
            printer.print_raw_token(&wc.keyword);
            printer.print_str(" ");
            printer.print(&wc.expr, shape.clone());
        }
        printer.print_str(" ");
        printer.print(&self.body, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.body.rightmost_token()
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
    pub body: StringLiteralValue,
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

        // body: a raw, quoted, or backtick string literal
        let body = StringLiteralValue::from_cst(it.expect_next("a template_string body")?)?;

        it.expect_end()?;

        Ok(TemplateStringDecl {
            keyword,
            name,
            args,
            body,
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
            .print(&self.body, Shape::unlimited_single_line())
            .multi_lined;
        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.body.rightmost_token()
    }
}

/// Corresponds to a [`SyntaxKind::TYPE_ALIAS_DEF`] node.
#[derive(Debug)]
pub struct TypeAliasDecl {
    pub keyword: t::TypeKw,
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

        // keyword: `type` (KW_TYPE)
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
