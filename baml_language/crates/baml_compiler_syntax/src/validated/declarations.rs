use super::{
    Attribute, BlockAttribute, BlockExpr, Expression, FromCST, GenericParamList, KnownKind,
    PathExpr, StrongAstError, SyntaxElement, SyntaxKind, SyntaxNodeIter, TextRange, ThrowsClause,
    Type, t,
};

validated_ast_data! {
    /// Any of the valid top-level declarations in a [`super::SourceFile`].
    #[allow(clippy::large_enum_variant)]
    pub enum TopLevelDeclaration {
        Function(FunctionDecl),
        Class(ClassDecl),
        Enum(EnumDecl),
        Client(ClientDecl),
        Test(TestDecl),
        TestExpr(TestExprDecl),
        TestSet(TestSetDecl),
        RetryPolicy(RetryPolicyDecl),
        TemplateString(TemplateStringDecl),
        TypeAlias(TypeAliasDecl),
        Generator(GeneratorDecl),
        Unknown(TextRange),
    }
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

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::FUNCTION_DEF`] node.
    FunctionDecl, FUNCTION_DEF {
        keyword: required t::Function;
        name: required t::Word;
        generic_params: optional GenericParamList;
        params: required FunctionParamList;
        arrow: required t::Arrow;
        return_type: required Type;
        throws: optional ThrowsClause;
        body: required FunctionDeclBody;
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::PARAMETER_LIST`] node.
    custom FunctionParamList, PARAMETER_LIST, parse_function_param_list {
        open_paren: t::LParen,
        params: Vec<(FunctionParam, Option<t::Comma>)>,
        close_paren: t::RParen,
    }
}

fn parse_function_param_list(elem: SyntaxElement) -> Result<FunctionParamList, StrongAstError> {
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

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::PARAMETER`] node.
    custom FunctionParam, PARAMETER, parse_function_param {
        name: t::Word,
        /// Type annotation with optional colon (colon is optional per BEP-019).
        ty: Option<(Option<t::Colon>, Type)>,
        default: Option<(t::Equals, Expression)>,
    }
}

fn parse_function_param(elem: SyntaxElement) -> Result<FunctionParam, StrongAstError> {
    let node = StrongAstError::assert_is_node(elem)?;
    StrongAstError::assert_kind_node(&node, SyntaxKind::PARAMETER)?;
    let mut it = SyntaxNodeIter::new(&node);
    let name = it.expect_parse()?;
    let colon = it
        .next_if_kind(SyntaxKind::COLON)
        .map(t::Colon::from_cst)
        .transpose()?;
    let ty = if colon.is_some() {
        let ty: Type = it.expect_parse()?;
        Some((colon, ty))
    } else if it.peek().map(SyntaxElement::kind) == Some(SyntaxKind::TYPE_EXPR) {
        let elem = it.next().expect("peeked");
        Some((None, Type::from_cst(elem)?))
    } else {
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

validated_ast_data! {
    /// Any of the valid function bodies in a [`FunctionDecl`].
    pub enum FunctionDeclBody {
        Llm(LlmFunctionBody),
        Block(BlockExpr),
    }
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

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::LLM_FUNCTION_BODY`] node.
    custom LlmFunctionBody, LLM_FUNCTION_BODY, parse_llm_function_body {
        open_brace: t::LBrace,
        /// Not guaranteed that client is before prompt in the input.
        client: ClientField,
        /// Not guaranteed that client is before prompt in the input.
        prompt: PromptField,
        /// Optional `type_builder { ... }` block for inline schema overrides.
        type_builder: Option<TypeBuilderBlock>,
        close_brace: t::RBrace,
    }
}

fn parse_llm_function_body(elem: SyntaxElement) -> Result<LlmFunctionBody, StrongAstError> {
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
    let type_builder = it
        .next_if_kind(SyntaxKind::TYPE_BUILDER_BLOCK)
        .map(TypeBuilderBlock::from_cst)
        .transpose()?;
    let close_brace = it.expect_parse()?;
    it.expect_end()?;
    Ok(LlmFunctionBody {
        open_brace,
        client,
        prompt,
        type_builder,
        close_brace,
    })
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::CLIENT_FIELD`] node.
    custom ClientField, CLIENT_FIELD, parse_client_field {
        keyword: t::Client,
        colon: Option<t::Colon>,
        name: ClientName,
    }
}

fn parse_client_field(elem: SyntaxElement) -> Result<ClientField, StrongAstError> {
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

validated_ast_data! {
    pub enum ClientName {
        Path(PathExpr),
        String(t::QuotedString),
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::PROMPT_FIELD`] node.
    PromptField, PROMPT_FIELD {
        prompt: required t::Word;
        colon: optional_element t::Colon;
        string: required StringLiteralValue;
    }
}

validated_ast_data! {
    /// A string-literal value as it appears in a declarative slot such as a
    /// [`PromptField`] or a [`TemplateStringDecl`]: a raw `#"..."#`, a quoted
    /// `"..."`, or a backtick `` `...` `` literal. All three parse equally in these
    /// positions, so the formatter accepts and re-emits any of them.
    pub enum StringLiteralValue {
        RawString(t::RawString),
        String(t::QuotedString),
        Backtick(t::BacktickString),
    }
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

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::CLASS_DEF`] node.
    custom ClassDecl, CLASS_DEF, parse_class_decl {
        keyword: t::Class,
        name: t::Word,
        generic_params: Option<GenericParamList>,
        open_brace: t::LBrace,
        items: Vec<ClassItem>,
        close_brace: t::RBrace,
    }
}

fn parse_class_decl(elem: SyntaxElement) -> Result<ClassDecl, StrongAstError> {
    let node = StrongAstError::assert_is_node(elem)?;
    StrongAstError::assert_kind_node(&node, SyntaxKind::CLASS_DEF)?;
    let mut it = SyntaxNodeIter::new(&node);
    let keyword = it.expect_parse()?;
    let name = it.expect_parse()?;
    let generic_params =
        if it.peek().map(SyntaxElement::kind) == Some(SyntaxKind::GENERIC_PARAM_LIST) {
            let elem = it.next().expect("peeked");
            Some(GenericParamList::from_cst(elem)?)
        } else {
            None
        };
    let open_brace = it.expect_parse()?;
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
    Ok(ClassDecl {
        keyword,
        name,
        generic_params,
        open_brace,
        items,
        close_brace,
    })
}

validated_ast_node! {
    ClassField, FIELD {
        name: required t::Word;
        colon: optional_element t::Colon;
        ty: required Type;
        attributes: rest Attribute;
    }
}

validated_ast_data! {
    /// Delimiter after a class field (comma or semicolon).
    /// The formatter normalizes to comma, but we preserve the original for trivia.
    pub enum ClassFieldDelimiter {
        Comma(t::Comma),
        Semicolon(t::Semicolon),
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::IMPLEMENTS_TARGET`] node.
    ImplementsTarget, IMPLEMENTS_TARGET {
        ty: required Type;
    }
}

validated_ast_data! {
    /// BEP-057 associated type declaration or implementation witness.
    pub struct AssociatedTypeDecl {
        pub keyword: t::TypeKw,
        pub name: t::Word,
        pub bound: Option<(t::Extends, Type)>,
        pub default: Option<(t::Equals, Type)>,
    }
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

validated_ast_data! {
    /// Corresponds to a [`SyntaxKind::INTERFACE_FIELD_LINK`] node.
    pub struct InterfaceFieldLink {
        pub interface_field: t::Word,
        pub as_token: t::As,
        pub class_field: t::Word,
    }
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

validated_ast_data! {
    /// Any item accepted inside a class `implements` block.
    #[allow(clippy::large_enum_variant)]
    pub enum ImplementsItem {
        AssociatedType(AssociatedTypeDecl, Option<ClassFieldDelimiter>),
        FieldLink(InterfaceFieldLink, Option<ClassFieldDelimiter>),
        Field(ClassField, Option<ClassFieldDelimiter>),
        Function(FunctionDecl),
    }
}

impl ImplementsItem {
    pub fn delimiter_rightmost(
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

validated_ast_data! {
    /// Corresponds to a [`SyntaxKind::IMPLEMENTS_BLOCK`] node.
    pub struct ImplementsBlock {
        pub keyword_span: TextRange,
        pub target: ImplementsTarget,
        pub open_brace: t::LBrace,
        pub items: Vec<ImplementsItem>,
        pub close_brace: t::RBrace,
    }
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

validated_ast_data! {
    /// Any of the valid items in a [`ClassDecl`].
    #[allow(clippy::large_enum_variant)]
    pub enum ClassItem {
        Field(ClassField, Option<ClassFieldDelimiter>),
        Function(FunctionDecl),
        Implements(ImplementsBlock),
        BlockAttribute(BlockAttribute),
        Unknown(TextRange),
    }
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

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::ENUM_DEF`] node.
    custom EnumDecl, ENUM_DEF, parse_enum_decl {
        keyword: t::Enum,
        name: t::Word,
        open_brace: t::LBrace,
        items: Vec<EnumItem>,
        close_brace: t::RBrace,
    }
}

fn parse_enum_decl(elem: SyntaxElement) -> Result<EnumDecl, StrongAstError> {
    let node = StrongAstError::assert_is_node(elem)?;
    StrongAstError::assert_kind_node(&node, SyntaxKind::ENUM_DEF)?;
    let enum_range = node.text_range();
    let mut it = SyntaxNodeIter::new(&node);
    let keyword = it.expect_parse()?;
    let name = it.expect_parse()?;
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

validated_ast_data! {
    /// Any of the valid items in an [`EnumDecl`].
    pub enum EnumItem {
        Variant(EnumVariant, Option<t::Comma>),
        BlockAttribute(BlockAttribute),
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::ENUM_VARIANT`] node.
    EnumVariant, ENUM_VARIANT {
        name: required t::Word;
        attributes: rest Attribute;
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::CLIENT_DEF`] node.
    ClientDecl, CLIENT_DEF {
        keyword: required t::Client;
        client_type: optional ClientType;
        name: required t::Word;
        config_block: required ConfigBlock;
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::CLIENT_TYPE`] node.
    ClientType, CLIENT_TYPE {
        langle: required t::Less;
        generic: required t::Word;
        rangle: required t::Greater;
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::CONFIG_BLOCK`] node.
    custom ConfigBlock, CONFIG_BLOCK, parse_config_block {
        open_brace: t::LBrace,
        items: Vec<(ConfigBlockMember, Option<t::Comma>)>,
        close_brace: t::RBrace,
    }
}

fn parse_config_block(elem: SyntaxElement) -> Result<ConfigBlock, StrongAstError> {
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
                    expected_desc: "CONFIG_ITEM, TYPE_BUILDER_BLOCK, BLOCK_ATTRIBUTE, or R_BRACE"
                        .into(),
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

validated_ast_data! {
    pub enum ConfigBlockMember {
        Item(ConfigItem),
        TypeBuilder(TypeBuilderBlock),
        BlockAttribute(BlockAttribute),
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::CONFIG_ITEM`] node.
    ConfigItem, CONFIG_ITEM {
        key: required ConfigItemKey;
        colon: optional_element t::Colon;
        value: required ConfigItemValue;
    }
}

validated_ast_data! {
    /// Any of the valid keys in a [`ConfigItem`].
    ///
    /// See `Parser::parse_config_item` in [`baml_db::baml_compiler_parser`]
    pub enum ConfigItemKey {
        Word(t::Word),
        String(t::QuotedString),
        RetryPolicy(t::RetryPolicy),
        Enum(t::Enum),
        Class(t::Class),
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

validated_ast_data! {
    /// Any of the valid values in a [`ConfigItem`].
    pub enum ConfigItemValue {
        Value(Expression),
        ConfigArray(ConfigArray),
        ConfigBlock(ConfigBlock),
    }
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
                    it.expect_end()?;
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

validated_ast_data! {
    /// Corresponds to a [`SyntaxKind::ARRAY_LITERAL`] node, when inside a [`ConfigBlock`].
    /// This is a special case because all elements will be [`ConfigItemValue`]s.
    pub struct ConfigArray {
        pub open_bracket: t::LBracket,
        pub elements: Vec<(ConfigItemValue, Option<t::Comma>)>,
        pub close_bracket: t::RBracket,
    }
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

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::TYPE_BUILDER_BLOCK`] node.
    custom TypeBuilderBlock, TYPE_BUILDER_BLOCK, parse_type_builder_block {
        keyword: t::TypeBuilder,
        open_brace: t::LBrace,
        items: Vec<TypeBuilderItem>,
        close_brace: t::RBrace,
    }
}

fn parse_type_builder_block(elem: SyntaxElement) -> Result<TypeBuilderBlock, StrongAstError> {
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

validated_ast_data! {
    /// Any of the valid items in a [`TypeBuilderBlock`].
    pub enum TypeBuilderItem {
        /// Corresponds to a [`SyntaxKind::DYNAMIC_TYPE_DEF`] node that containins a class definition.
        DynamicClass(t::Dynamic, ClassDecl),
        /// Corresponds to a [`SyntaxKind::DYNAMIC_TYPE_DEF`] node that containins an enum definition.
        DynamicEnum(t::Dynamic, EnumDecl),
        Class(ClassDecl),
        Enum(EnumDecl),
        TypeAlias(TypeAliasDecl),
    }
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

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::TEST_DEF`] node.
    TestDecl, TEST_DEF {
        keyword: required t::Test;
        name: required t::Word;
        config_block: required ConfigBlock;
    }
}

validated_ast_data! {
    /// The `with <expr>` clause on test/testset declarations.
    pub struct WithClause {
        pub keyword: t::With,
        pub expr: Expression,
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::TEST_EXPR_DEF`] node.
    custom TestExprDecl, TEST_EXPR_DEF, parse_test_expr_decl {
        keyword: t::Test,
        /// Test name - any expression that evaluates to a string. The parser
        /// accepts string literals, raw strings, identifiers, concatenations,
        /// arithmetic, etc.; type-checking enforces the string requirement.
        name: Expression,
        with_clause: Option<WithClause>,
        body: BlockExpr,
    }
}

fn parse_test_expr_decl(elem: SyntaxElement) -> Result<TestExprDecl, StrongAstError> {
    let node = StrongAstError::assert_is_node(elem)?;
    StrongAstError::assert_kind_node(&node, SyntaxKind::TEST_EXPR_DEF)?;
    let mut it = SyntaxNodeIter::new(&node);
    let keyword = it.expect_parse()?;
    let name_elem = it.expect_next("a test name expression")?;
    let name = Expression::from_cst(name_elem)?;
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
    let body: BlockExpr = it.expect_parse()?;
    it.expect_end()?;
    Ok(TestExprDecl {
        keyword,
        name,
        with_clause,
        body,
    })
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::TESTSET_DEF`] node.
    custom TestSetDecl, TESTSET_DEF, parse_test_set_decl {
        keyword: t::TestSet,
        /// Testset name - any expression (string literal, raw string, identifier,
        /// concatenation, etc.); type-checking enforces the string requirement.
        name: Expression,
        with_clause: Option<WithClause>,
        body: BlockExpr,
    }
}

fn parse_test_set_decl(elem: SyntaxElement) -> Result<TestSetDecl, StrongAstError> {
    let node = StrongAstError::assert_is_node(elem)?;
    StrongAstError::assert_kind_node(&node, SyntaxKind::TESTSET_DEF)?;
    let mut it = SyntaxNodeIter::new(&node);
    let keyword = it.expect_parse()?;
    let name_elem = it.expect_next("a testset name expression")?;
    let name = Expression::from_cst(name_elem)?;
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
    let body: BlockExpr = it.expect_parse()?;
    it.expect_end()?;
    Ok(TestSetDecl {
        keyword,
        name,
        with_clause,
        body,
    })
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::RETRY_POLICY_DEF`] node.
    RetryPolicyDecl, RETRY_POLICY_DEF {
        keyword: required t::RetryPolicy;
        name: required t::Word;
        config_block: required ConfigBlock;
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::TEMPLATE_STRING_DEF`] node.
    TemplateStringDecl, TEMPLATE_STRING_DEF {
        keyword: required t::TemplateString;
        name: required t::Word;
        args: required FunctionParamList;
        body: required StringLiteralValue;
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::TYPE_ALIAS_DEF`] node.
    TypeAliasDecl, TYPE_ALIAS_DEF {
        keyword: required t::TypeKw;
        name: required t::Word;
        equals: required t::Equals;
        type_expr: required Type;
        semicolon: optional_element t::Semicolon;
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::GENERATOR_DEF`] node.
    GeneratorDecl, GENERATOR_DEF {
        keyword: required t::Generator;
        name: required t::Word;
        config: required ConfigBlock;
    }
}
