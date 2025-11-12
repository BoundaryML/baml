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
            .nth(1) // Skip the "function" keyword, get the second WORD
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
            .nth(1) // Skip the "class" keyword, get the second WORD
    }

    /// Get all fields.
    pub fn fields(&self) -> impl Iterator<Item = Field> {
        self.syntax.children().filter_map(Field::cast)
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
