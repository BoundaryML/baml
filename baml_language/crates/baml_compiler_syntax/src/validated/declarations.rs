use super::{
    AttachedSeparatedUntil, Attribute, BlockAttribute, BlockExpr, ClassFieldDelimiter, Expression,
    FromCST, GenericParamList, KnownKind, OptionalPrefixed, OptionalPrefixedOrBare, PathExpr,
    SeparatedUntil, SyntaxElement, SyntaxKind, SyntaxNodeIter, TextRange, ThrowsClause, Type,
    Until, ValidatedAstError, WithSeparator, t,
};

validated_ast_enum! {
    /// Any of the valid top-level declarations in a [`super::SourceFile`].
    #[allow(clippy::large_enum_variant)]
    pub enum TopLevelDeclaration {
        FUNCTION_DEF => Function(FunctionDecl),
        CLASS_DEF => Class(ClassDecl),
        ENUM_DEF => Enum(EnumDecl),
        CLIENT_DEF => Client(ClientDecl),
        TEST_DEF => Test(TestDecl),
        TEST_EXPR_DEF => TestExpr(TestExprDecl),
        TESTSET_DEF => TestSet(TestSetDecl),
        RETRY_POLICY_DEF => RetryPolicy(RetryPolicyDecl),
        TEMPLATE_STRING_DEF => TemplateString(TemplateStringDecl),
        TYPE_ALIAS_DEF => TypeAlias(TypeAliasDecl),
        GENERATOR_DEF => Generator(GeneratorDecl),
        _ => Unknown,
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
    FunctionParamList, PARAMETER_LIST {
        open_paren: required t::LParen;
        params: spec SeparatedUntil<FunctionParam, t::Comma, t::RParen>;
        close_paren: required t::RParen;
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::PARAMETER`] node.
    FunctionParam, PARAMETER {
        name: required t::Word;
        /// Type annotation with optional colon (colon is optional per BEP-019).
        ty: spec OptionalPrefixedOrBare<t::Colon, Type>;
        default: spec OptionalPrefixed<t::Equals, Expression>;
    }
}

validated_ast_data! {
    /// Any of the valid function bodies in a [`FunctionDecl`].
    pub enum FunctionDeclBody {
        Llm(LlmFunctionBody),
        Block(BlockExpr),
    }
}

impl FromCST for FunctionDeclBody {
    fn from_cst(elem: SyntaxElement) -> Result<Self, ValidatedAstError> {
        let node = ValidatedAstError::assert_is_node(elem)?;
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
            _ => Err(ValidatedAstError::UnexpectedKindDesc {
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

fn parse_llm_function_body(elem: SyntaxElement) -> Result<LlmFunctionBody, ValidatedAstError> {
    let node = ValidatedAstError::assert_is_node(elem)?;
    ValidatedAstError::assert_kind_node(&node, SyntaxKind::LLM_FUNCTION_BODY)?;
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
            return Err(ValidatedAstError::UnexpectedKindDesc {
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

fn parse_client_field(elem: SyntaxElement) -> Result<ClientField, ValidatedAstError> {
    let node = ValidatedAstError::assert_is_node(elem)?;
    ValidatedAstError::assert_kind_node(&node, SyntaxKind::CLIENT_FIELD)?;
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
            return Err(ValidatedAstError::UnexpectedKindDesc {
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

validated_ast_enum! {
    /// A string-literal value as it appears in a declarative slot such as a
    /// [`PromptField`] or a [`TemplateStringDecl`]: a raw `#"..."#`, a quoted
    /// `"..."`, or a backtick `` `...` `` literal. All three parse equally in these
    /// positions, so the formatter accepts and re-emits any of them.
    pub enum StringLiteralValue {
        RAW_STRING_LITERAL => RawString(t::RawString),
        STRING_LITERAL => String(t::QuotedString),
        BACKTICK_STRING_LITERAL => Backtick(t::BacktickString),
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::CLASS_DEF`] node.
    ClassDecl, CLASS_DEF {
        keyword: required t::Class;
        name: required t::Word;
        generic_params: optional GenericParamList;
        open_brace: required t::LBrace;
        items: spec AttachedSeparatedUntil<ClassItem, ClassFieldDelimiter, t::RBrace>;
        close_brace: required t::RBrace;
    }
}

validated_ast_node! {
    ClassField, FIELD {
        name: required t::Word;
        colon: optional_element t::Colon;
        ty: required Type;
        attributes: rest Attribute;
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
    fn from_cst(elem: SyntaxElement) -> Result<Self, ValidatedAstError> {
        let node = ValidatedAstError::assert_is_node(elem)?;
        ValidatedAstError::assert_kind_node(&node, SyntaxKind::ASSOCIATED_TYPE_DECL)?;
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
                    return Err(ValidatedAstError::UnexpectedAdditionalElement {
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

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::INTERFACE_FIELD_LINK`] node.
    InterfaceFieldLink, INTERFACE_FIELD_LINK {
        interface_field: required t::Word;
        as_token: required t::As;
        class_field: required t::Word;
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

impl WithSeparator<ClassFieldDelimiter> for ImplementsItem {
    fn with_separator(self, separator: Option<ClassFieldDelimiter>) -> Self {
        match self {
            Self::AssociatedType(decl, _) => Self::AssociatedType(decl, separator),
            Self::FieldLink(link, _) => Self::FieldLink(link, separator),
            Self::Field(field, _) => Self::Field(field, separator),
            Self::Function(function) => Self::Function(function),
        }
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

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::IMPLEMENTS_BLOCK`] node.
    ImplementsBlock, IMPLEMENTS_BLOCK {
        keyword: required t::ImplementsKeyword;
        target: required ImplementsTarget;
        open_brace: required t::LBrace;
        items: spec AttachedSeparatedUntil<ImplementsItem, ClassFieldDelimiter, t::RBrace>;
        close_brace: required t::RBrace;
    }
}

impl FromCST for ImplementsItem {
    fn from_cst(elem: SyntaxElement) -> Result<Self, ValidatedAstError> {
        match elem.kind() {
            SyntaxKind::ASSOCIATED_TYPE_DECL => Ok(Self::AssociatedType(
                AssociatedTypeDecl::from_cst(elem)?,
                None,
            )),
            SyntaxKind::INTERFACE_FIELD_LINK => {
                Ok(Self::FieldLink(InterfaceFieldLink::from_cst(elem)?, None))
            }
            SyntaxKind::FIELD => Ok(Self::Field(ClassField::from_cst(elem)?, None)),
            SyntaxKind::FUNCTION_DEF => Ok(Self::Function(FunctionDecl::from_cst(elem)?)),
            found => Err(ValidatedAstError::UnexpectedKindDesc {
                expected_desc: "ASSOCIATED_TYPE_DECL, INTERFACE_FIELD_LINK, FIELD, or FUNCTION_DEF"
                    .into(),
                found,
                at: elem.text_range(),
            }),
        }
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
    fn from_cst(elem: SyntaxElement) -> Result<Self, ValidatedAstError> {
        let item = match elem.kind() {
            SyntaxKind::FIELD => ClassItem::Field(ClassField::from_cst(elem)?, None),
            SyntaxKind::FUNCTION_DEF => ClassItem::Function(FunctionDecl::from_cst(elem)?),
            SyntaxKind::IMPLEMENTS_BLOCK => ClassItem::Implements(ImplementsBlock::from_cst(elem)?),
            SyntaxKind::BLOCK_ATTRIBUTE => {
                ClassItem::BlockAttribute(BlockAttribute::from_cst(elem)?)
            }
            found => {
                return Err(ValidatedAstError::UnexpectedKindDesc {
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

impl WithSeparator<ClassFieldDelimiter> for ClassItem {
    fn with_separator(self, separator: Option<ClassFieldDelimiter>) -> Self {
        match self {
            Self::Field(field, _) => Self::Field(field, separator),
            item => item,
        }
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::ENUM_DEF`] node.
    EnumDecl, ENUM_DEF {
        keyword: required t::Enum;
        name: required t::Word;
        open_brace: required t::LBrace;
        items: spec AttachedSeparatedUntil<EnumItem, t::Comma, t::RBrace>;
        close_brace: required t::RBrace;
    }
}

validated_ast_data! {
    /// Any of the valid items in an [`EnumDecl`].
    pub enum EnumItem {
        Variant(EnumVariant, Option<t::Comma>),
        BlockAttribute(BlockAttribute),
    }
}

impl FromCST for EnumItem {
    fn from_cst(elem: SyntaxElement) -> Result<Self, ValidatedAstError> {
        match elem.kind() {
            SyntaxKind::ENUM_VARIANT => Ok(Self::Variant(EnumVariant::from_cst(elem)?, None)),
            SyntaxKind::BLOCK_ATTRIBUTE => {
                Ok(Self::BlockAttribute(BlockAttribute::from_cst(elem)?))
            }
            found => Err(ValidatedAstError::UnexpectedKindDesc {
                expected_desc: "ENUM_VARIANT or BLOCK_ATTRIBUTE".into(),
                found,
                at: elem.text_range(),
            }),
        }
    }
}

impl WithSeparator<t::Comma> for EnumItem {
    fn with_separator(self, separator: Option<t::Comma>) -> Self {
        match self {
            Self::Variant(variant, _) => Self::Variant(variant, separator),
            item @ Self::BlockAttribute(_) => item,
        }
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
    ConfigBlock, CONFIG_BLOCK {
        open_brace: required t::LBrace;
        items: spec SeparatedUntil<ConfigBlockMember, t::Comma, t::RBrace>;
        close_brace: required t::RBrace;
    }
}

validated_ast_enum! {
    pub enum ConfigBlockMember {
        CONFIG_ITEM => Item(ConfigItem),
        TYPE_BUILDER_BLOCK => TypeBuilder(TypeBuilderBlock),
        BLOCK_ATTRIBUTE => BlockAttribute(BlockAttribute),
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

validated_ast_enum! {
    /// Any of the valid keys in a [`ConfigItem`].
    ///
    /// See `Parser::parse_config_item` in `baml_db::baml_compiler_parser`.
    pub enum ConfigItemKey {
        WORD => Word(t::Word),
        STRING_LITERAL => String(t::QuotedString),
        KW_RETRY_POLICY => RetryPolicy(t::RetryPolicy),
        KW_ENUM => Enum(t::Enum),
        KW_CLASS => Class(t::Class),
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
    fn from_cst(elem: SyntaxElement) -> Result<Self, ValidatedAstError> {
        let node = ValidatedAstError::assert_is_node(elem)?;
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
            _ => Err(ValidatedAstError::UnexpectedKindDesc {
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
    fn from_cst(elem: SyntaxElement) -> Result<Self, ValidatedAstError> {
        let node = ValidatedAstError::assert_is_node(elem)?;
        ValidatedAstError::assert_kind_node(&node, SyntaxKind::ARRAY_LITERAL)?;
        let mut it = SyntaxNodeIter::new(&node);
        let open_bracket = it.expect_parse()?;
        let mut elements = Vec::new();
        let close_bracket = loop {
            let Some(elem) = it.next() else {
                return Err(ValidatedAstError::missing(SyntaxKind::R_BRACKET, it.parent));
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
    TypeBuilderBlock, TYPE_BUILDER_BLOCK {
        keyword: required t::TypeBuilder;
        open_brace: required t::LBrace;
        items: spec Until<TypeBuilderItem, t::RBrace>;
        close_brace: required t::RBrace;
    }
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
    fn from_cst(elem: SyntaxElement) -> Result<Self, ValidatedAstError> {
        match elem.kind() {
            SyntaxKind::DYNAMIC_TYPE_DEF => {
                let node = ValidatedAstError::assert_is_node(elem)?;
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
                    _ => Err(ValidatedAstError::UnexpectedKindDesc {
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
            _ => Err(ValidatedAstError::UnexpectedKindDesc {
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

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::TEST_EXPR_DEF`] node.
    TestExprDecl, TEST_EXPR_DEF {
        keyword: required t::Test;
        /// Test name - any expression that evaluates to a string. The parser
        /// accepts string literals, raw strings, identifiers, concatenations,
        /// arithmetic, etc.; type-checking enforces the string requirement.
        name: required Expression;
        with_clause: spec OptionalPrefixed<t::With, Expression>;
        body: required BlockExpr;
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::TESTSET_DEF`] node.
    TestSetDecl, TESTSET_DEF {
        keyword: required t::TestSet;
        /// Testset name - any expression (string literal, raw string, identifier,
        /// concatenation, etc.); type-checking enforces the string requirement.
        name: required Expression;
        with_clause: spec OptionalPrefixed<t::With, Expression>;
        body: required BlockExpr;
    }
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
