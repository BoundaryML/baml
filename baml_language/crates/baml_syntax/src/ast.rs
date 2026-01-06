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
ast_node!(GeneratorDef, GENERATOR_DEF);
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

impl TypeExpr {
    /// Check if this is a union type (contains PIPE separators).
    ///
    /// Returns `true` for types like `Success | Failure` or `"user" | "assistant"`.
    pub fn is_union(&self) -> bool {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .any(|t| t.kind() == SyntaxKind::PIPE)
    }

    /// Get the text parts of this type expression, split by PIPE separators.
    ///
    /// For union types like `A | B | C`, returns `["A", "B", "C"]`.
    /// For non-union types like `string?`, returns `["string?"]`.
    ///
    /// Each part is trimmed of surrounding whitespace.
    pub fn parts(&self) -> Vec<String> {
        let mut parts = Vec::new();
        let mut current_part = String::new();

        for child in self.syntax.children_with_tokens() {
            match child {
                rowan::NodeOrToken::Token(token) => {
                    // Skip trivia tokens (whitespace, comments)
                    if token.kind().is_trivia() {
                        continue;
                    }
                    if token.kind() == SyntaxKind::PIPE {
                        let trimmed = current_part.trim().to_string();
                        if !trimmed.is_empty() {
                            parts.push(trimmed);
                        }
                        current_part = String::new();
                    } else {
                        current_part.push_str(token.text());
                    }
                }
                rowan::NodeOrToken::Node(child_node) => {
                    // For nested nodes (like TYPE_ARGS), include their full text
                    current_part.push_str(&child_node.text().to_string());
                }
            }
        }

        // Include the final part
        let trimmed = current_part.trim().to_string();
        if !trimmed.is_empty() {
            parts.push(trimmed);
        }

        parts
    }
}
ast_node!(BlockAttribute, BLOCK_ATTRIBUTE);

ast_node!(Expr, EXPR);

// LetStmt accepts both LET_STMT and WATCH_LET syntax kinds
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LetStmt {
    syntax: SyntaxNode,
}

impl BamlAstNode for LetStmt {}

impl AstNode for LetStmt {
    type Language = crate::BamlLanguage;

    fn can_cast(kind: <Self::Language as rowan::Language>::Kind) -> bool {
        kind == SyntaxKind::LET_STMT || kind == SyntaxKind::WATCH_LET
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

ast_node!(IfExpr, IF_EXPR);
ast_node!(WhileStmt, WHILE_STMT);
ast_node!(ForExpr, FOR_EXPR);
ast_node!(BlockExpr, BLOCK_EXPR);
ast_node!(ReturnStmt, RETURN_STMT);
ast_node!(BreakStmt, BREAK_STMT);
ast_node!(ContinueStmt, CONTINUE_STMT);
ast_node!(PathExpr, PATH_EXPR);
ast_node!(FieldAccessExpr, FIELD_ACCESS_EXPR);
ast_node!(MatchExpr, MATCH_EXPR);
ast_node!(MatchArm, MATCH_ARM);
ast_node!(MatchPattern, MATCH_PATTERN);
ast_node!(MatchGuard, MATCH_GUARD);

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

impl GeneratorDef {
    /// Get the generator name.
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| {
                token.kind() == SyntaxKind::WORD && token.parent() == Some(self.syntax.clone())
            })
            .nth(0) // Get the first WORD (generator keyword is KW_GENERATOR, not WORD)
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

    /// Get the config item value (WORD token inside `CONFIG_VALUE`, if present).
    /// For simple `key value` patterns like `provider openai`.
    /// The value is nested inside a `CONFIG_VALUE` node: `CONFIG_ITEM` { WORD "key", `CONFIG_VALUE` { WORD "value" } }
    pub fn value_word(&self) -> Option<SyntaxToken> {
        // Find the CONFIG_VALUE child node
        self.syntax
            .children()
            .find(|child| child.kind() == SyntaxKind::CONFIG_VALUE)
            .and_then(|config_value| {
                // Look for a WORD token inside CONFIG_VALUE
                config_value
                    .children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .find(|token| token.kind() == SyntaxKind::WORD)
            })
    }

    /// Get the text range of the config value, regardless of whether it's a WORD or `STRING_LITERAL`.
    /// This is useful for error reporting when the value type doesn't matter.
    pub fn value_text_range(&self) -> Option<rowan::TextRange> {
        self.syntax
            .children()
            .find(|child| child.kind() == SyntaxKind::CONFIG_VALUE)
            .map(|config_value| config_value.text_range())
    }

    /// Get the full config item value as a string.
    /// This handles compound values like "python/pydantic" that span multiple tokens.
    /// Returns the unquoted text of the value.
    pub fn value_str(&self) -> Option<String> {
        self.syntax
            .children()
            .find(|child| child.kind() == SyntaxKind::CONFIG_VALUE)
            .map(|config_value| {
                // Collect all non-whitespace, non-quote token text from nested tokens
                config_value
                    .descendants_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .filter(|token| {
                        !matches!(
                            token.kind(),
                            SyntaxKind::WHITESPACE
                                | SyntaxKind::NEWLINE
                                | SyntaxKind::LINE_COMMENT
                                | SyntaxKind::BLOCK_COMMENT
                                | SyntaxKind::QUOTE
                        )
                    })
                    .map(|token| token.text().to_string())
                    .collect::<String>()
            })
            .filter(|s| !s.is_empty())
    }

    /// Get a nested config block, if this item has one.
    /// For items like `options { ... }` or `http { ... }`.
    pub fn nested_block(&self) -> Option<ConfigBlock> {
        self.syntax.children().find_map(ConfigBlock::cast)
    }

    /// Check if this config item has a `CONFIG_VALUE` child (vs a nested `CONFIG_BLOCK`).
    pub fn has_value(&self) -> bool {
        self.syntax
            .children()
            .any(|child| child.kind() == SyntaxKind::CONFIG_VALUE)
    }

    /// Get the integer value if this is an integer literal.
    pub fn value_int(&self) -> Option<i64> {
        self.syntax
            .children()
            .find(|child| child.kind() == SyntaxKind::CONFIG_VALUE)
            .and_then(|config_value| {
                config_value
                    .descendants_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .find(|token| token.kind() == SyntaxKind::INTEGER_LITERAL)
                    .and_then(|token| token.text().parse().ok())
            })
    }

    /// Check if the value starts with a minus sign (for negative numbers).
    pub fn is_negative(&self) -> bool {
        self.syntax
            .children()
            .find(|child| child.kind() == SyntaxKind::CONFIG_VALUE)
            .map(|config_value| {
                config_value
                    .descendants_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .any(|token| token.kind() == SyntaxKind::MINUS)
            })
            .unwrap_or(false)
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

    /// Get the function name that this test is for (first function only).
    /// Pattern: `test <TestName> { functions [<FunctionName>] ... }`
    pub fn function_name(&self) -> Option<SyntaxToken> {
        self.function_names().into_iter().next()
    }

    /// Get all function names that this test is for.
    /// Pattern: `test <TestName> { functions [<Func1>, <Func2>, ...] ... }`
    pub fn function_names(&self) -> Vec<SyntaxToken> {
        // Look for a ConfigItem with key "functions" and extract all function names
        // The function names are inside a CONFIG_VALUE node: functions [Func1, Func2]
        self.syntax
            .descendants()
            .filter_map(ConfigItem::cast)
            .find(|item| item.key().map(|k| k.text() == "functions").unwrap_or(false))
            .map(|item| {
                // Look for WORD tokens within the config item's descendants
                // Skip the first one (which is the key "functions")
                item.syntax()
                    .descendants_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .filter(|token| token.kind() == SyntaxKind::WORD)
                    .skip(1) // Skip "functions" key
                    .collect()
            })
            .unwrap_or_default()
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
    /// Get the first segment of the attribute name (e.g., "dynamic" from @@dynamic).
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| matches!(token.kind(), SyntaxKind::WORD | SyntaxKind::KW_DYNAMIC))
    }

    /// Get the full attribute name including dot-separated modifiers.
    /// For @@stream.done returns "stream.done", for @@dynamic returns "dynamic".
    pub fn full_name(&self) -> Option<String> {
        let segments: Vec<String> = self
            .syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| matches!(token.kind(), SyntaxKind::WORD | SyntaxKind::KW_DYNAMIC))
            .map(|token| token.text().to_string())
            .collect();

        if segments.is_empty() {
            None
        } else {
            Some(segments.join("."))
        }
    }

    /// Get the text range covering the full attribute name (including modifiers).
    pub fn full_name_range(&self) -> Option<rowan::TextRange> {
        let tokens: Vec<_> = self
            .syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| {
                matches!(
                    token.kind(),
                    SyntaxKind::WORD | SyntaxKind::KW_DYNAMIC | SyntaxKind::DOT
                )
            })
            .collect();

        if tokens.is_empty() {
            return None;
        }

        let first = tokens.first()?;
        let last = tokens.last()?;

        Some(rowan::TextRange::new(
            first.text_range().start(),
            last.text_range().end(),
        ))
    }
}

impl Attribute {
    /// Get the first segment of the attribute name (e.g., "stream" from @stream.done).
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| token.kind() == SyntaxKind::WORD)
    }

    /// Get the full attribute name including dot-separated modifiers.
    /// For @stream.done returns "stream.done", for @alias returns "alias".
    pub fn full_name(&self) -> Option<String> {
        let segments: Vec<String> = self
            .syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| token.kind() == SyntaxKind::WORD)
            .map(|token| token.text().to_string())
            .collect();

        if segments.is_empty() {
            None
        } else {
            Some(segments.join("."))
        }
    }

    /// Get the text range covering the full attribute name (including modifiers).
    /// For @stream.done returns the range from "stream" to "done".
    pub fn full_name_range(&self) -> Option<rowan::TextRange> {
        let tokens: Vec<_> = self
            .syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| matches!(token.kind(), SyntaxKind::WORD | SyntaxKind::DOT))
            .collect();

        if tokens.is_empty() {
            return None;
        }

        let first = tokens.first()?;
        let last = tokens.last()?;

        Some(rowan::TextRange::new(
            first.text_range().start(),
            last.text_range().end(),
        ))
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
    /// This is the expression between the first and second semicolon.
    pub fn condition(&self) -> Option<SyntaxNode> {
        if self.is_iterator_style() {
            return None;
        }
        // For C-style, condition is after the initializer's semicolon, before the second semicolon.
        // Note: If there's a LET_STMT, its trailing semicolon is INSIDE the LET_STMT node.
        // So for `for (let i = 0; ; update)`:
        //   - LET_STMT contains `let i = 0;` (first semicolon inside)
        //   - Sibling semicolon (second semicolon overall)
        //   - update expression
        // For `for (; cond; update)`:
        //   - First sibling semicolon
        //   - condition expression
        //   - Second sibling semicolon
        //   - update expression

        let has_initializer = self.let_stmt().is_some();
        let mut sibling_semicolon_count = 0;

        for element in self.syntax.children_with_tokens() {
            match element {
                rowan::NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::SEMICOLON {
                        sibling_semicolon_count += 1;
                    }
                }
                rowan::NodeOrToken::Node(node) => {
                    // Skip LET_STMT (initializer) and BLOCK_EXPR (body)
                    if matches!(node.kind(), SyntaxKind::LET_STMT | SyntaxKind::BLOCK_EXPR) {
                        continue;
                    }
                    // Condition position depends on whether there's an initializer:
                    // - With initializer: after LET_STMT, before first sibling semicolon
                    // - Without initializer: after first sibling semicolon, before second
                    let condition_position = i32::from(!has_initializer);
                    if sibling_semicolon_count == condition_position {
                        return Some(node);
                    }
                }
            }
        }
        None
    }

    /// Get a bare token as condition (for C-style loops like `for (; false;)`).
    /// Used when `condition()` returns None but there's a literal token between semicolons.
    pub fn condition_token(&self) -> Option<SyntaxToken> {
        if self.is_iterator_style() {
            return None;
        }
        // Only look for tokens if there's no expression node
        if self.condition().is_some() {
            return None;
        }

        // Condition position depends on whether there's an initializer.
        // With initializer (LET_STMT contains first semicolon):
        //   condition is BEFORE first sibling semicolon
        // Without initializer:
        //   condition is AFTER first sibling semicolon, BEFORE second

        let has_initializer = self.let_stmt().is_some();
        let mut sibling_semicolon_count = 0;
        let mut after_let_stmt = !has_initializer;

        for element in self.syntax.children_with_tokens() {
            match element {
                rowan::NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::SEMICOLON {
                        sibling_semicolon_count += 1;
                        // Check if we're past the condition position
                        if has_initializer && sibling_semicolon_count >= 1 {
                            return None; // Past condition position for initializer case
                        }
                        if !has_initializer && sibling_semicolon_count >= 2 {
                            return None; // Past condition position for no-initializer case
                        }
                    } else if after_let_stmt {
                        // Check for condition token
                        let in_condition_position = if has_initializer {
                            sibling_semicolon_count == 0
                        } else {
                            sibling_semicolon_count == 1
                        };
                        if in_condition_position {
                            match token.kind() {
                                SyntaxKind::WORD
                                | SyntaxKind::INTEGER_LITERAL
                                | SyntaxKind::FLOAT_LITERAL => {
                                    return Some(token);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                rowan::NodeOrToken::Node(node) => {
                    if node.kind() == SyntaxKind::LET_STMT {
                        after_let_stmt = true;
                    }
                }
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
        // For C-style, update is after the condition's semicolon.
        // Note: If there's a LET_STMT, its trailing semicolon is INSIDE the LET_STMT node.
        // So for `for (let i = 0; ; update)`:
        //   - LET_STMT contains first semicolon
        //   - Sibling semicolon count 1 marks end of condition position
        //   - update expression is at sibling_semicolon_count == 1
        // For `for (; cond; update)`:
        //   - update expression is at sibling_semicolon_count == 2

        let has_initializer = self.let_stmt().is_some();
        let mut sibling_semicolon_count = 0;

        for element in self.syntax.children_with_tokens() {
            match element {
                rowan::NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::SEMICOLON {
                        sibling_semicolon_count += 1;
                    }
                }
                rowan::NodeOrToken::Node(node) => {
                    // Skip LET_STMT (initializer) and BLOCK_EXPR (body)
                    if matches!(node.kind(), SyntaxKind::LET_STMT | SyntaxKind::BLOCK_EXPR) {
                        continue;
                    }
                    // Update position depends on whether there's an initializer:
                    // - With initializer: after first sibling semicolon
                    // - Without initializer: after second sibling semicolon
                    let update_position = if has_initializer { 1 } else { 2 };
                    if sibling_semicolon_count == update_position {
                        return Some(node);
                    }
                }
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
                    | SyntaxKind::MAP_LITERAL
                    | SyntaxKind::STRING_LITERAL
                    | SyntaxKind::RAW_STRING_LITERAL
            )
        })
    }

    /// Get the initializer as a token (for direct literals like integers, bools, null).
    /// Returns the literal token if the initializer is a simple literal.
    pub fn initializer_token(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| {
                match token.kind() {
                    SyntaxKind::INTEGER_LITERAL
                    | SyntaxKind::FLOAT_LITERAL
                    | SyntaxKind::STRING_LITERAL
                    | SyntaxKind::RAW_STRING_LITERAL => true,
                    // Boolean and null literals are parsed as WORD tokens
                    SyntaxKind::WORD => matches!(token.text(), "true" | "false" | "null"),
                    _ => false,
                }
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
    /// A header comment (`//# name`)
    HeaderComment(SyntaxNode),
}

impl BlockElement {
    /// Returns true if this element is a statement (has no value).
    pub fn is_stmt(&self) -> bool {
        matches!(self, BlockElement::Stmt(_) | BlockElement::HeaderComment(_))
    }

    /// Returns true if this element is an expression (has a value).
    pub fn is_expr(&self) -> bool {
        matches!(self, BlockElement::ExprNode(_) | BlockElement::ExprToken(_))
    }

    /// Get the syntax node if this is a node-based element.
    pub fn as_node(&self) -> Option<&SyntaxNode> {
        match self {
            BlockElement::Stmt(n) | BlockElement::ExprNode(n) | BlockElement::HeaderComment(n) => {
                Some(n)
            }
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
            BlockElement::HeaderComment(_) => false, // Header comments don't have trailing semicolons
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
                        | SyntaxKind::WATCH_LET
                        | SyntaxKind::RETURN_STMT
                        | SyntaxKind::WHILE_STMT
                        | SyntaxKind::FOR_EXPR
                        | SyntaxKind::BREAK_STMT
                        | SyntaxKind::CONTINUE_STMT
                        | SyntaxKind::ASSERT_STMT => Some(BlockElement::Stmt(n)),
                        // Header comment (//# name)
                        SyntaxKind::HEADER_COMMENT => Some(BlockElement::HeaderComment(n)),
                        // Expression nodes
                        SyntaxKind::EXPR
                        | SyntaxKind::BINARY_EXPR
                        | SyntaxKind::UNARY_EXPR
                        | SyntaxKind::CALL_EXPR
                        | SyntaxKind::IF_EXPR
                        | SyntaxKind::MATCH_EXPR
                        | SyntaxKind::BLOCK_EXPR
                        | SyntaxKind::PATH_EXPR
                        | SyntaxKind::FIELD_ACCESS_EXPR
                        | SyntaxKind::INDEX_EXPR
                        | SyntaxKind::PAREN_EXPR
                        | SyntaxKind::ARRAY_LITERAL
                        | SyntaxKind::OBJECT_LITERAL
                        | SyntaxKind::MAP_LITERAL
                        | SyntaxKind::STRING_LITERAL
                        | SyntaxKind::RAW_STRING_LITERAL => Some(BlockElement::ExprNode(n)),
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

impl MatchExpr {
    /// Get the scrutinee expression (the value being matched).
    ///
    /// For `match (expr) { ... }`, returns the expression inside parentheses.
    pub fn scrutinee(&self) -> Option<SyntaxNode> {
        // The scrutinee is the first child node (expression between parentheses)
        self.syntax.children().next()
    }

    /// Iterate over all match arms.
    pub fn arms(&self) -> impl Iterator<Item = MatchArm> + '_ {
        self.syntax.children().filter_map(MatchArm::cast)
    }
}

impl MatchArm {
    /// Get the pattern for this arm.
    pub fn pattern(&self) -> Option<MatchPattern> {
        self.syntax.children().find_map(MatchPattern::cast)
    }

    /// Get the guard expression, if present.
    ///
    /// For `pattern if condition => body`, returns the `if condition` part.
    pub fn guard(&self) -> Option<MatchGuard> {
        self.syntax.children().find_map(MatchGuard::cast)
    }

    /// Get the body expression of this arm.
    ///
    /// The body is the expression after `=>`. It can be a simple expression
    /// or a block expression.
    pub fn body(&self) -> Option<SyntaxNode> {
        // The body is the last child node that is an expression (not pattern or guard)
        // Find the fat arrow and return the expression after it
        let mut found_fat_arrow = false;
        for element in self.syntax.children_with_tokens() {
            match element {
                rowan::NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::FAT_ARROW {
                        found_fat_arrow = true;
                    }
                }
                rowan::NodeOrToken::Node(node) => {
                    if found_fat_arrow {
                        return Some(node);
                    }
                }
            }
        }
        None
    }

    /// Check if the body is a block expression.
    pub fn has_block_body(&self) -> bool {
        self.body()
            .map(|n| n.kind() == SyntaxKind::BLOCK_EXPR)
            .unwrap_or(false)
    }
}

impl MatchPattern {
    /// Check if this is a union pattern (has `|` separators).
    pub fn is_union(&self) -> bool {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .any(|token| token.kind() == SyntaxKind::PIPE)
    }

    /// Check if this is a typed binding pattern (has `:`).
    ///
    /// For patterns like `s: Success`, returns true.
    pub fn is_typed_binding(&self) -> bool {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .any(|token| token.kind() == SyntaxKind::COLON)
    }

    /// Check if this is an enum variant pattern (has `.`).
    ///
    /// For patterns like `Status.Active`, returns true.
    pub fn is_enum_variant(&self) -> bool {
        // An enum variant has a dot but NOT a colon (typed binding)
        let has_dot = self
            .syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .any(|token| token.kind() == SyntaxKind::DOT);
        has_dot && !self.is_typed_binding()
    }

    /// Check if this is a wildcard pattern (`_`).
    pub fn is_wildcard(&self) -> bool {
        let tokens: Vec<_> = self
            .syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|t| t.kind() == SyntaxKind::WORD)
            .collect();
        tokens.len() == 1 && tokens[0].text() == "_"
    }

    /// Get the binding name if this is a binding pattern.
    ///
    /// For `s: Success`, returns "s".
    /// For `x`, returns "x".
    /// For `_`, returns "_".
    pub fn binding_name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| token.kind() == SyntaxKind::WORD)
    }

    /// Get the type expression if this is a typed binding pattern.
    ///
    /// For `s: Success`, returns the `Success` type expression.
    pub fn binding_type(&self) -> Option<TypeExpr> {
        self.syntax.children().find_map(TypeExpr::cast)
    }

    /// Get all identifiers in this pattern.
    ///
    /// For simple patterns, returns one identifier.
    /// For enum variants like `Status.Active`, returns both identifiers (e.g. "Status", "Active").
    /// For union patterns, returns identifiers from all branches.
    pub fn identifiers(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| token.kind() == SyntaxKind::WORD)
    }

    /// Get the literal token if this is a literal pattern.
    ///
    /// Returns the token for integer, float, or string literals,
    /// as well as `null`, `true`, `false` keywords (which are parsed as WORD).
    pub fn literal(&self) -> Option<SyntaxToken> {
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
                ) || (token.kind() == SyntaxKind::WORD
                    && matches!(token.text(), "null" | "true" | "false"))
            })
    }

    /// Get all pattern elements for union patterns.
    ///
    /// For `Success | Failure`, returns iterator over the token groups
    /// representing each alternative in the union.
    ///
    /// Note: This is a simplified view. For complex union patterns,
    /// you may need to manually iterate `children_with_tokens()` and
    /// split on `PIPE` tokens.
    pub fn union_elements(&self) -> Vec<Vec<SyntaxToken>> {
        let mut elements = Vec::new();
        let mut current = Vec::new();

        for element in self.syntax.children_with_tokens() {
            if let Some(token) = element.into_token() {
                if token.kind() == SyntaxKind::PIPE {
                    if !current.is_empty() {
                        elements.push(std::mem::take(&mut current));
                    }
                } else if !token.kind().is_trivia() {
                    current.push(token);
                }
            }
        }

        if !current.is_empty() {
            elements.push(current);
        }

        elements
    }
}

impl MatchGuard {
    /// Get the condition expression.
    ///
    /// For `if condition`, returns the condition expression.
    pub fn condition(&self) -> Option<SyntaxNode> {
        self.syntax.children().next()
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
