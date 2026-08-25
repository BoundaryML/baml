use crate::{
    SyntaxElement, SyntaxKind, TextRange,
    validated::{
        FromCST, KnownKind, StrongAstError, SyntaxNodeIter, ValidatedToken as Token,
        nodes::{Attribute, BlockAttribute, BlockExpr, Expression, PathExpr, ThrowsClause, Type},
        tokens as t,
    },
};

/// Any of the valid top-level declarations in a [`super::SourceFile`].
#[derive(Debug)]
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
            // a wrapper, ...) - print through the expression machinery.
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

#[derive(Debug)]
pub enum ClientName {
    Path(PathExpr),
    String(t::QuotedString),
    /// An arbitrary ai.Client expression (`client: openai.ResponsesClient.new(...)`).
    Expr(Box<Expression>),
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
    pub fn span(&self) -> TextRange {
        match self {
            Self::Comma(comma) => comma.span(),
            Self::Semicolon(semicolon) => semicolon.span(),
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

#[derive(Debug)]
pub enum ConfigBlockMember {
    Item(ConfigItem),
    BlockAttribute(BlockAttribute),
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

/// Any of the valid keys in a [`ConfigItem`].
///
/// See `Parser::parse_config_item` in `baml_compiler_parser`.
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
    /// Test name - any expression that evaluates to a string. The parser
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

        // name - any expression
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

/// Corresponds to a [`SyntaxKind::TESTSET_DEF`] node.
#[derive(Debug)]
pub struct TestSetDecl {
    pub keyword: t::TestSet,
    /// Testset name - any expression (string literal, raw string, identifier,
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

        // name - any expression
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
