//! Typed AST node wrappers for ergonomic tree access.

use rowan::ast::AstNode;

use crate::{SyntaxKind, SyntaxNode, SyntaxToken};

/// Trait for all AST nodes.
pub trait BamlAstNode: AstNode<Language = crate::BamlLanguage> {
    /// Get the syntax kind of this node.
    fn kind(&self) -> SyntaxKind {
        self.syntax().kind()
    }
}

/// Macro to define AST node types.
macro_rules! ast_node {
    ($name:ident, $kind:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name {
            syntax: SyntaxNode,
        }

        impl BamlAstNode for $name {}

        impl AstNode for $name {
            type Language = crate::BamlLanguage;

            fn can_cast(kind: <Self::Language as rowan::Language>::Kind) -> bool {
                kind == SyntaxKind::$kind.into()
            }

            fn cast(syntax: SyntaxNode) -> Option<Self> {
                if Self::can_cast(syntax.kind()) {
                    Some(Self { syntax })
                } else {
                    None
                }
            }

            fn syntax(&self) -> &SyntaxNode {
                &self.syntax
            }
        }
    };
}

// Define all AST node types
ast_node!(SourceFile, SOURCE_FILE);
ast_node!(FunctionDef, FUNCTION_DEF);
ast_node!(ClassDef, CLASS_DEF);
ast_node!(EnumDef, ENUM_DEF);
ast_node!(ClientDef, CLIENT_DEF);
ast_node!(TestDef, TEST_DEF);
ast_node!(RetryPolicyDef, RETRY_POLICY_DEF);
ast_node!(TemplateStringDef, TEMPLATE_STRING_DEF);
ast_node!(TypeAliasDef, TYPE_ALIAS_DEF);

ast_node!(ParameterList, PARAMETER_LIST);
ast_node!(Parameter, PARAMETER);
ast_node!(FunctionBody, FUNCTION_BODY);
ast_node!(LlmFunctionBody, LLM_FUNCTION_BODY);
ast_node!(ExprFunctionBody, EXPR_FUNCTION_BODY);
ast_node!(Field, FIELD);
ast_node!(EnumVariant, ENUM_VARIANT);
ast_node!(ConfigBlock, CONFIG_BLOCK);
ast_node!(ConfigItem, CONFIG_ITEM);

ast_node!(TypeExpr, TYPE_EXPR);
ast_node!(Attribute, ATTRIBUTE);
ast_node!(BlockAttribute, BLOCK_ATTRIBUTE);

ast_node!(Expr, EXPR);
ast_node!(LetStmt, LET_STMT);
ast_node!(IfExpr, IF_EXPR);
ast_node!(WhileStmt, WHILE_STMT);
ast_node!(ForExpr, FOR_EXPR);
ast_node!(BlockExpr, BLOCK_EXPR);
ast_node!(ReturnStmt, RETURN_STMT);
ast_node!(BreakStmt, BREAK_STMT);
ast_node!(ContinueStmt, CONTINUE_STMT);
ast_node!(PathExpr, PATH_EXPR);
ast_node!(FieldAccessExpr, FIELD_ACCESS_EXPR);

// Implement accessor methods
impl SourceFile {
    /// Iterate over all top-level items in the file.
    pub fn items(&self) -> impl Iterator<Item = Item> {
        self.syntax.children().filter_map(Item::cast)
    }
}

impl FunctionDef {
    /// Get the function name.
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| {
                token.kind() == SyntaxKind::WORD && token.parent() == Some(self.syntax.clone())
            })
            .nth(0) // Get the first WORD (function keyword is KW_FUNCTION, not WORD)
    }

    /// Get the parameter list.
    pub fn param_list(&self) -> Option<ParameterList> {
        self.syntax.children().find_map(ParameterList::cast)
    }

    /// Get the return type.
    pub fn return_type(&self) -> Option<TypeExpr> {
        self.syntax.children().find_map(TypeExpr::cast)
    }

    /// Get the function body (generic, could be any type).
    pub fn body(&self) -> Option<FunctionBody> {
        self.syntax.children().find_map(FunctionBody::cast)
    }

    /// Get the LLM function body if this is an LLM function.
    pub fn llm_body(&self) -> Option<LlmFunctionBody> {
        self.syntax.children().find_map(LlmFunctionBody::cast)
    }

    /// Get the expression function body if this is an expression function.
    pub fn expr_body(&self) -> Option<ExprFunctionBody> {
        self.syntax.children().find_map(ExprFunctionBody::cast)
    }

    /// Check if this is an LLM function.
    pub fn is_llm_function(&self) -> bool {
        self.llm_body().is_some()
    }

    /// Check if this is an expression function.
    pub fn is_expr_function(&self) -> bool {
        self.expr_body().is_some()
    }
}

impl Parameter {
    /// Get the parameter name.
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| token.kind() == SyntaxKind::WORD)
    }

    /// Get the parameter type.
    pub fn ty(&self) -> Option<TypeExpr> {
        self.syntax.children().find_map(TypeExpr::cast)
    }
}

impl ParameterList {
    /// Get all parameters.
    pub fn params(&self) -> impl Iterator<Item = Parameter> {
        self.syntax.children().filter_map(Parameter::cast)
    }
}

impl ClassDef {
    /// Get the class name.
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| {
                token.kind() == SyntaxKind::WORD && token.parent() == Some(self.syntax.clone())
            })
            .nth(0) // Get the first WORD (class keyword is KW_CLASS, not WORD)
    }

    /// Get all fields.
    pub fn fields(&self) -> impl Iterator<Item = Field> {
        self.syntax.children().filter_map(Field::cast)
    }

    /// Get all methods (function definitions inside the class).
    pub fn methods(&self) -> impl Iterator<Item = FunctionDef> {
        self.syntax.children().filter_map(FunctionDef::cast)
    }

    /// Get block attributes (@@dynamic).
    pub fn block_attributes(&self) -> impl Iterator<Item = BlockAttribute> {
        self.syntax.children().filter_map(BlockAttribute::cast)
    }
}

impl Field {
    /// Get the field name.
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| token.kind() == SyntaxKind::WORD)
    }

    /// Get the field type.
    pub fn ty(&self) -> Option<TypeExpr> {
        self.syntax.children().find_map(TypeExpr::cast)
    }

    /// Get field attributes (@alias, @description, etc.).
    pub fn attributes(&self) -> impl Iterator<Item = Attribute> {
        self.syntax.children().filter_map(Attribute::cast)
    }
}

impl EnumDef {
    /// Get the enum name.
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| {
                token.kind() == SyntaxKind::WORD && token.parent() == Some(self.syntax.clone())
            })
            .nth(0) // Get the first WORD (enum keyword is KW_ENUM, not WORD)
    }

    /// Check if this enum has a body (braces).
    /// Malformed enums from error recovery may not have braces.
    pub fn has_body(&self) -> bool {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .any(|t| t.kind() == SyntaxKind::L_BRACE)
    }

    /// Get all variants.
    pub fn variants(&self) -> impl Iterator<Item = EnumVariant> {
        self.syntax.children().filter_map(EnumVariant::cast)
    }

    /// Get block attributes.
    pub fn block_attributes(&self) -> impl Iterator<Item = BlockAttribute> {
        self.syntax.children().filter_map(BlockAttribute::cast)
    }
}

impl EnumVariant {
    /// Get the variant name.
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| token.kind() == SyntaxKind::WORD)
    }

    /// Get variant attributes (@alias, @description, etc.).
    pub fn attributes(&self) -> impl Iterator<Item = Attribute> {
        self.syntax.children().filter_map(Attribute::cast)
    }
}

impl ClientDef {
    /// Get the client name.
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| {
                token.kind() == SyntaxKind::WORD && token.parent() == Some(self.syntax.clone())
            })
            .nth(0) // Get the first WORD (client keyword is KW_CLIENT, not WORD)
    }

    /// Get the config block.
    pub fn config_block(&self) -> Option<ConfigBlock> {
        self.syntax.children().find_map(ConfigBlock::cast)
    }
}

impl ConfigBlock {
    /// Get all config items.
    pub fn items(&self) -> impl Iterator<Item = ConfigItem> {
        self.syntax.children().filter_map(ConfigItem::cast)
    }
}

impl ConfigItem {
    /// Get the config item key (first WORD token).
    pub fn key(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| token.kind() == SyntaxKind::WORD)
    }

    /// Get the config item value (second WORD token, if present).
    /// For simple `key value` patterns like `provider openai`.
    pub fn value_word(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| token.kind() == SyntaxKind::WORD)
            .nth(1)
    }
}

impl TestDef {
    /// Get the test name.
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| {
                token.kind() == SyntaxKind::WORD && token.parent() == Some(self.syntax.clone())
            })
            .nth(0) // Get the first WORD (test keyword is KW_TEST, not WORD)
    }

    /// Get the function name that this test is for.
    /// Pattern: `test <TestName> { functions [<FunctionName>] ... }`
    pub fn function_name(&self) -> Option<SyntaxToken> {
        // Look for a ConfigItem with key "functions" and extract the function name
        self.syntax
            .descendants()
            .filter_map(ConfigItem::cast)
            .find(|item| item.key().map(|k| k.text() == "functions").unwrap_or(false))
            .and_then(|item| item.value_word())
    }

    /// Get the config block.
    pub fn config_block(&self) -> Option<ConfigBlock> {
        self.syntax.children().find_map(ConfigBlock::cast)
    }
}

impl TypeAliasDef {
    /// Get the type alias name.
    /// Note: "type" is parsed as a WORD token, not a keyword, so we skip it.
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| {
                token.kind() == SyntaxKind::WORD && token.parent() == Some(self.syntax.clone())
            })
            .nth(1) // Skip "type" keyword (which is a WORD), get the actual name
    }

    /// Get the aliased type expression.
    pub fn ty(&self) -> Option<TypeExpr> {
        self.syntax.children().find_map(TypeExpr::cast)
    }
}

impl BlockAttribute {
    /// Get the attribute name (e.g., "dynamic" from @@dynamic).
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| matches!(token.kind(), SyntaxKind::WORD | SyntaxKind::KW_DYNAMIC))
    }
}

impl Attribute {
    /// Get the attribute name (e.g., "alias" from @alias).
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| token.kind() == SyntaxKind::WORD)
    }
}

impl WhileStmt {
    /// Get the condition expression.
    /// The condition is the first child expression of the while statement.
    pub fn condition(&self) -> Option<SyntaxNode> {
        self.syntax.children().next()
    }

    /// Get the body block expression.
    /// The body is the second child (`BLOCK_EXPR`) of the while statement.
    pub fn body(&self) -> Option<BlockExpr> {
        self.syntax.children().find_map(BlockExpr::cast)
    }
}

impl IfExpr {
    /// Get the condition expression.
    /// The condition is the first child expression of the if expression.
    pub fn condition(&self) -> Option<SyntaxNode> {
        self.syntax.children().next()
    }

    /// Get the then branch block expression.
    /// The then branch is the first `BLOCK_EXPR` child.
    pub fn then_branch(&self) -> Option<BlockExpr> {
        self.syntax.children().find_map(BlockExpr::cast)
    }

    /// Get the else branch, which could be another `IfExpr` (else if) or a `BlockExpr` (else).
    pub fn else_branch(&self) -> Option<SyntaxNode> {
        let children: Vec<_> = self.syntax.children().collect();
        // If there are more than 2 children, the third one is the else branch
        children.get(2).cloned()
    }
}

impl ForExpr {
    /// Check if this is an iterator-style for loop (has 'in' keyword).
    pub fn is_iterator_style(&self) -> bool {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .any(|token| token.kind() == SyntaxKind::KW_IN)
    }

    /// Get the let statement (initializer) if present.
    /// Used for both `for (let i in ...)` and `for (let i = 0; ...)`.
    pub fn let_stmt(&self) -> Option<LetStmt> {
        self.syntax.children().find_map(LetStmt::cast)
    }

    /// Get the loop variable name (for simple `for i in ...` without let).
    pub fn loop_var(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| token.kind() == SyntaxKind::WORD)
    }

    /// Get the iterator expression (for iterator-style loops).
    /// This is the expression after `in` keyword.
    pub fn iterator(&self) -> Option<SyntaxNode> {
        // Find expression after the 'in' keyword
        // The iterator is not a LET_STMT and not a BLOCK_EXPR
        self.syntax
            .children()
            .find(|n| !matches!(n.kind(), SyntaxKind::LET_STMT | SyntaxKind::BLOCK_EXPR))
    }

    /// Get the condition expression (for C-style loops).
    /// This is the expression after the first semicolon.
    pub fn condition(&self) -> Option<SyntaxNode> {
        if self.is_iterator_style() {
            return None;
        }
        // For C-style, the condition is the second expression child (after LET_STMT)
        let mut found_let = false;
        for child in self.syntax.children() {
            if child.kind() == SyntaxKind::LET_STMT {
                found_let = true;
                continue;
            }
            if found_let && child.kind() != SyntaxKind::BLOCK_EXPR {
                return Some(child);
            }
        }
        None
    }

    /// Get the update expression (for C-style loops).
    /// This is the expression after the second semicolon.
    pub fn update(&self) -> Option<SyntaxNode> {
        if self.is_iterator_style() {
            return None;
        }
        // For C-style, the update is the third expression child
        let mut count = 0;
        for child in self.syntax.children() {
            if child.kind() == SyntaxKind::LET_STMT {
                continue;
            }
            if child.kind() == SyntaxKind::BLOCK_EXPR {
                continue;
            }
            count += 1;
            if count == 2 {
                return Some(child);
            }
        }
        None
    }

    /// Get the body block expression.
    pub fn body(&self) -> Option<BlockExpr> {
        self.syntax.children().find_map(BlockExpr::cast)
    }
}

impl LetStmt {
    /// Get the variable name.
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| token.kind() == SyntaxKind::WORD)
    }

    /// Get the type annotation, if present.
    pub fn ty(&self) -> Option<TypeExpr> {
        self.syntax.children().find_map(TypeExpr::cast)
    }

    /// Get the initializer expression as a node.
    /// This finds the first child node that is an expression (not `TYPE_EXPR`).
    pub fn initializer(&self) -> Option<SyntaxNode> {
        self.syntax.children().find(|n| {
            matches!(
                n.kind(),
                SyntaxKind::EXPR
                    | SyntaxKind::BINARY_EXPR
                    | SyntaxKind::UNARY_EXPR
                    | SyntaxKind::CALL_EXPR
                    | SyntaxKind::PATH_EXPR
                    | SyntaxKind::FIELD_ACCESS_EXPR
                    | SyntaxKind::INDEX_EXPR
                    | SyntaxKind::IF_EXPR
                    | SyntaxKind::BLOCK_EXPR
                    | SyntaxKind::PAREN_EXPR
                    | SyntaxKind::ARRAY_LITERAL
                    | SyntaxKind::OBJECT_LITERAL
                    | SyntaxKind::STRING_LITERAL
                    | SyntaxKind::RAW_STRING_LITERAL
            )
        })
    }

    /// Get the initializer as a token (for direct literals like integers).
    /// Returns the literal token if the initializer is a simple literal.
    pub fn initializer_token(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| {
                matches!(
                    token.kind(),
                    SyntaxKind::INTEGER_LITERAL
                        | SyntaxKind::FLOAT_LITERAL
                        | SyntaxKind::STRING_LITERAL
                        | SyntaxKind::RAW_STRING_LITERAL
                )
            })
    }
}

impl ReturnStmt {
    /// Get the return value expression, if present.
    pub fn value(&self) -> Option<SyntaxNode> {
        self.syntax.children().next()
    }
}

/// An element within a block expression - either a statement node or an expression token.
#[derive(Debug, Clone)]
pub enum BlockElement {
    /// A statement node (`LET_STMT`, `RETURN_STMT`, `WHILE_STMT`, `FOR_EXPR`)
    Stmt(SyntaxNode),
    /// An expression node (various expression kinds)
    ExprNode(SyntaxNode),
    /// A literal or identifier token that forms an expression
    ExprToken(SyntaxToken),
}

impl BlockElement {
    /// Returns true if this element is a statement (has no value).
    pub fn is_stmt(&self) -> bool {
        matches!(self, BlockElement::Stmt(_))
    }

    /// Returns true if this element is an expression (has a value).
    pub fn is_expr(&self) -> bool {
        matches!(self, BlockElement::ExprNode(_) | BlockElement::ExprToken(_))
    }

    /// Get the syntax node if this is a node-based element.
    pub fn as_node(&self) -> Option<&SyntaxNode> {
        match self {
            BlockElement::Stmt(n) | BlockElement::ExprNode(n) => Some(n),
            BlockElement::ExprToken(_) => None,
        }
    }

    /// Get the syntax token if this is a token-based element.
    pub fn as_token(&self) -> Option<&SyntaxToken> {
        match self {
            BlockElement::ExprToken(t) => Some(t),
            _ => None,
        }
    }

    /// Check if this element has a trailing semicolon.
    pub fn has_trailing_semicolon(&self) -> bool {
        use rowan::Direction;

        match self {
            BlockElement::Stmt(node) | BlockElement::ExprNode(node) => {
                node.siblings_with_tokens(Direction::Next)
                    .skip(1) // Skip the node itself
                    .filter_map(rowan::NodeOrToken::into_token)
                    .any(|token| token.kind() == SyntaxKind::SEMICOLON)
            }
            BlockElement::ExprToken(token) => {
                // For tokens, check siblings
                token
                    .siblings_with_tokens(Direction::Next)
                    .skip(1)
                    .filter_map(rowan::NodeOrToken::into_token)
                    .any(|t| t.kind() == SyntaxKind::SEMICOLON)
            }
        }
    }
}

impl BlockExpr {
    /// Iterate over all significant elements in this block (statements and expressions).
    ///
    /// This filters out braces, whitespace, and other structural tokens, returning
    /// only the meaningful content of the block.
    pub fn elements(&self) -> impl Iterator<Item = BlockElement> + '_ {
        self.syntax.children_with_tokens().filter_map(|el| {
            match el {
                rowan::NodeOrToken::Node(n) => {
                    match n.kind() {
                        // Statement nodes
                        SyntaxKind::LET_STMT
                        | SyntaxKind::RETURN_STMT
                        | SyntaxKind::WHILE_STMT
                        | SyntaxKind::FOR_EXPR => Some(BlockElement::Stmt(n)),
                        // Expression nodes
                        SyntaxKind::EXPR
                        | SyntaxKind::BINARY_EXPR
                        | SyntaxKind::UNARY_EXPR
                        | SyntaxKind::CALL_EXPR
                        | SyntaxKind::IF_EXPR
                        | SyntaxKind::BLOCK_EXPR
                        | SyntaxKind::PATH_EXPR
                        | SyntaxKind::FIELD_ACCESS_EXPR
                        | SyntaxKind::INDEX_EXPR
                        | SyntaxKind::PAREN_EXPR
                        | SyntaxKind::ARRAY_LITERAL
                        | SyntaxKind::OBJECT_LITERAL => Some(BlockElement::ExprNode(n)),
                        _ => None,
                    }
                }
                rowan::NodeOrToken::Token(t) => {
                    // Keep literals and identifiers (potential tail expressions)
                    match t.kind() {
                        SyntaxKind::WORD
                        | SyntaxKind::INTEGER_LITERAL
                        | SyntaxKind::FLOAT_LITERAL
                        | SyntaxKind::STRING_LITERAL
                        | SyntaxKind::RAW_STRING_LITERAL => Some(BlockElement::ExprToken(t)),
                        _ => None,
                    }
                }
            }
        })
    }
}

impl PathExpr {
    /// Check if this path contains dots (field access syntax).
    pub fn has_dots(&self) -> bool {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .any(|token| token.kind() == SyntaxKind::DOT)
    }

    /// Get all segments of this path (the WORD tokens).
    /// For `foo.bar.baz`, returns `["foo", "bar", "baz"]`.
    /// For `mod.func`, returns `["mod", "func"]`.
    pub fn segments(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| token.kind() == SyntaxKind::WORD)
    }
}

impl FieldAccessExpr {
    /// Get the base expression being accessed.
    pub fn base(&self) -> Option<SyntaxNode> {
        self.syntax.children().next()
    }

    /// Get the field name being accessed.
    pub fn field(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| token.kind() == SyntaxKind::WORD)
            .last() // The field name is the last WORD token
    }
}

/// Enum for any top-level item.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Item {
    Function(FunctionDef),
    Class(ClassDef),
    Enum(EnumDef),
    Client(ClientDef),
    Test(TestDef),
    RetryPolicy(RetryPolicyDef),
    TemplateString(TemplateStringDef),
    TypeAlias(TypeAliasDef),
}

impl AstNode for Item {
    type Language = crate::BamlLanguage;

    fn can_cast(kind: <Self::Language as rowan::Language>::Kind) -> bool {
        matches!(
            kind,
            SyntaxKind::FUNCTION_DEF
                | SyntaxKind::CLASS_DEF
                | SyntaxKind::ENUM_DEF
                | SyntaxKind::CLIENT_DEF
                | SyntaxKind::TEST_DEF
                | SyntaxKind::RETRY_POLICY_DEF
                | SyntaxKind::TEMPLATE_STRING_DEF
                | SyntaxKind::TYPE_ALIAS_DEF
        )
    }

    fn cast(syntax: SyntaxNode) -> Option<Self> {
        match syntax.kind() {
            SyntaxKind::FUNCTION_DEF => Some(Item::Function(FunctionDef { syntax })),
            SyntaxKind::CLASS_DEF => Some(Item::Class(ClassDef { syntax })),
            SyntaxKind::ENUM_DEF => Some(Item::Enum(EnumDef { syntax })),
            SyntaxKind::CLIENT_DEF => Some(Item::Client(ClientDef { syntax })),
            SyntaxKind::TEST_DEF => Some(Item::Test(TestDef { syntax })),
            SyntaxKind::RETRY_POLICY_DEF => Some(Item::RetryPolicy(RetryPolicyDef { syntax })),
            SyntaxKind::TEMPLATE_STRING_DEF => {
                Some(Item::TemplateString(TemplateStringDef { syntax }))
            }
            SyntaxKind::TYPE_ALIAS_DEF => Some(Item::TypeAlias(TypeAliasDef { syntax })),
            _ => None,
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        match self {
            Item::Function(it) => it.syntax(),
            Item::Class(it) => it.syntax(),
            Item::Enum(it) => it.syntax(),
            Item::Client(it) => it.syntax(),
            Item::Test(it) => it.syntax(),
            Item::RetryPolicy(it) => it.syntax(),
            Item::TemplateString(it) => it.syntax(),
            Item::TypeAlias(it) => it.syntax(),
        }
    }
}
