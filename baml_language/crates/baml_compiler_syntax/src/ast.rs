//! Typed AST node wrappers for ergonomic tree access.

use rowan::ast::AstNode;

use crate::{SyntaxKind, SyntaxNode, SyntaxToken};

/// Extract a dotted name from a token sequence (e.g., `baml.http.Request` → `"baml.http.Request"`).
///
/// Finds the first WORD token, then consumes alternating DOT + WORD pairs.
fn extract_dotted_name<'a>(tokens: impl Iterator<Item = &'a SyntaxToken>) -> Option<String> {
    let mut parts = Vec::new();
    let mut iter = tokens.filter(|t| !t.kind().is_trivia());

    // Find first WORD
    let first = loop {
        match iter.next() {
            Some(t) if t.kind() == SyntaxKind::WORD => break t,
            Some(_) => continue,
            None => return None,
        }
    };
    parts.push(first.text().to_string());

    // Consume alternating DOT + WORD
    while let Some(t) = iter.next() {
        if t.kind() != SyntaxKind::DOT {
            break;
        }
        let Some(word) = iter.next() else { break };
        if word.kind() != SyntaxKind::WORD {
            break;
        }
        parts.push(word.text().to_string());
    }

    Some(parts.join("."))
}

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
ast_node!(ClientField, CLIENT_FIELD);
ast_node!(PromptField, PROMPT_FIELD);
ast_node!(RawStringLiteral, RAW_STRING_LITERAL);
ast_node!(StringLiteral, STRING_LITERAL);

// Jinja template components (inside raw strings)
ast_node!(JinjaExpression, TEMPLATE_INTERPOLATION);
ast_node!(JinjaStatement, TEMPLATE_CONTROL);
ast_node!(JinjaComment, TEMPLATE_COMMENT);
ast_node!(PromptText, PROMPT_TEXT);

ast_node!(TypeExpr, TYPE_EXPR);
ast_node!(Attribute, ATTRIBUTE);
ast_node!(TypeBuilderBlock, TYPE_BUILDER_BLOCK);
ast_node!(DynamicTypeDef, DYNAMIC_TYPE_DEF);

/// Parts of a union member for token-based parsing.
///
/// Union members can contain both tokens (WORD, `L_BRACKET`, etc.) and child nodes
/// (`STRING_LITERAL`, `TYPE_EXPR` for parenthesized types, `TYPE_ARGS` for generics).
#[derive(Debug, Clone)]
pub struct UnionMemberParts {
    /// Tokens in this union member (WORD, `L_BRACKET`, `R_BRACKET`, QUESTION, etc.).
    pub tokens: Vec<SyntaxToken>,
    /// Child nodes in this union member (`STRING_LITERAL`, `TYPE_EXPR`, `TYPE_ARGS`, etc.).
    pub child_nodes: Vec<SyntaxNode>,
}

impl UnionMemberParts {
    /// Create an empty `UnionMemberParts`.
    pub fn new() -> Self {
        Self {
            tokens: Vec::new(),
            child_nodes: Vec::new(),
        }
    }

    /// Check if this member is empty (no tokens or child nodes).
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty() && self.child_nodes.is_empty()
    }

    /// Get the first WORD token's text, if any.
    pub fn first_word(&self) -> Option<&str> {
        self.tokens
            .iter()
            .find(|t| t.kind() == SyntaxKind::WORD)
            .map(rowan::SyntaxToken::text)
    }

    /// Get the full dotted name (all WORD tokens joined by DOTs).
    ///
    /// For `baml.http.Request` returns `Some("baml.http.Request")`.
    /// For `MyClass` returns `Some("MyClass")`.
    pub fn dotted_name(&self) -> Option<String> {
        extract_dotted_name(self.tokens.iter())
    }

    /// Check if this member has a trailing `?` (optional modifier).
    pub fn is_optional(&self) -> bool {
        self.tokens
            .last()
            .is_some_and(|t| t.kind() == SyntaxKind::QUESTION)
    }

    /// Count the number of `[]` array modifiers at the end.
    pub fn array_depth(&self) -> usize {
        let mut depth = 0;
        let mut i = self.tokens.len();

        // Skip trailing ? if present
        if i > 0 && self.tokens[i - 1].kind() == SyntaxKind::QUESTION {
            i -= 1;
        }

        // Count [] pairs from the end
        while i >= 2 {
            if self.tokens[i - 1].kind() == SyntaxKind::R_BRACKET
                && self.tokens[i - 2].kind() == SyntaxKind::L_BRACKET
            {
                depth += 1;
                i -= 2;
            } else {
                break;
            }
        }

        depth
    }

    /// Check if this member contains a `STRING_LITERAL` child node.
    pub fn has_string_literal(&self) -> bool {
        self.child_nodes
            .iter()
            .any(|n| n.kind() == SyntaxKind::STRING_LITERAL)
    }

    /// Get the string literal value if this member is a string literal type.
    pub fn string_literal(&self) -> Option<String> {
        self.child_nodes
            .iter()
            .find(|n| n.kind() == SyntaxKind::STRING_LITERAL)
            .map(|n| {
                let text = n.text().to_string();
                let trimmed = text.trim();
                if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
                    trimmed[1..trimmed.len() - 1].to_string()
                } else {
                    trimmed.trim_start_matches('"').to_string()
                }
            })
    }

    /// Check if this member contains a `TYPE_EXPR` child node (parenthesized type).
    pub fn has_type_expr(&self) -> bool {
        self.child_nodes
            .iter()
            .any(|n| n.kind() == SyntaxKind::TYPE_EXPR)
    }

    /// Get the `TYPE_EXPR` child node if present (for parenthesized types).
    pub fn type_expr(&self) -> Option<TypeExpr> {
        self.child_nodes
            .iter()
            .find(|n| n.kind() == SyntaxKind::TYPE_EXPR)
            .cloned()
            .map(|syntax| TypeExpr { syntax })
    }

    /// Get the `TYPE_ARGS` child node if present (for generic types like map<K,V>).
    pub fn type_args(&self) -> Option<SyntaxNode> {
        self.child_nodes
            .iter()
            .find(|n| n.kind() == SyntaxKind::TYPE_ARGS)
            .cloned()
    }

    /// Get the `FUNCTION_TYPE_PARAM` child node if present.
    ///
    /// This is used for parenthesized types like `(Union | Union)` which have
    /// `L_PAREN`, `FUNCTION_TYPE_PARAM`, `R_PAREN` as direct children.
    pub fn function_type_param(&self) -> Option<SyntaxNode> {
        self.child_nodes
            .iter()
            .find(|n| n.kind() == SyntaxKind::FUNCTION_TYPE_PARAM)
            .cloned()
    }

    /// Check if this member has an `INTEGER_LITERAL` token.
    pub fn integer_literal(&self) -> Option<i64> {
        self.tokens
            .iter()
            .find(|t| t.kind() == SyntaxKind::INTEGER_LITERAL)
            .and_then(|t| t.text().parse().ok())
    }
}

impl Default for UnionMemberParts {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeExpr {
    /// Check if this is a union type (contains top-level PIPE separators).
    ///
    /// Returns `true` for types like `Success | Failure` or `int[] | string[]`.
    /// Returns `false` for `(int | string)[]` because the PIPE is inside parens.
    pub fn is_union(&self) -> bool {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .any(|t| t.kind() == SyntaxKind::PIPE)
    }

    /// Check if this type has a trailing `?` (optional modifier).
    pub fn is_optional(&self) -> bool {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .last()
            .is_some_and(|t| t.kind() == SyntaxKind::QUESTION)
    }

    /// Check if this type has trailing `[]` (array modifier).
    ///
    /// For `int[]?`, this returns true (array comes before optional).
    pub fn is_array(&self) -> bool {
        self.array_depth() > 0
    }

    /// Count the number of `[]` array modifiers.
    ///
    /// For `int` returns 0.
    /// For `int[]` returns 1.
    /// For `int[][]` returns 2.
    /// For `int[]?` returns 1 (optional is separate).
    pub fn array_depth(&self) -> usize {
        let tokens: Vec<_> = self
            .syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|t| !t.kind().is_trivia())
            .collect();

        let mut depth = 0;
        let mut i = tokens.len();

        // Skip trailing ? if present
        if i > 0 && tokens[i - 1].kind() == SyntaxKind::QUESTION {
            i -= 1;
        }

        // Count [] pairs from the end
        while i >= 2 {
            if tokens[i - 1].kind() == SyntaxKind::R_BRACKET
                && tokens[i - 2].kind() == SyntaxKind::L_BRACKET
            {
                depth += 1;
                i -= 2;
            } else {
                break;
            }
        }

        depth
    }

    /// Check if this type is wrapped in parentheses (e.g., `(int | string)`).
    pub fn is_parenthesized(&self) -> bool {
        let first_token = self
            .syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| !t.kind().is_trivia());

        first_token.is_some_and(|t| t.kind() == SyntaxKind::L_PAREN)
    }

    /// Get the inner `TypeExpr` for parenthesized types like `(int | string)`.
    ///
    /// Returns None if this is not a parenthesized type or if it's a function type.
    /// For function types, use `function_type_params()` and `function_return_type()` instead.
    pub fn inner_type_expr(&self) -> Option<TypeExpr> {
        if !self.is_parenthesized() {
            return None;
        }
        // If this is a function type, don't return the inner type
        // (use function_type_params/function_return_type instead)
        if self.is_function_type() {
            return None;
        }

        // First, try to find a direct TYPE_EXPR child (legacy structure)
        if let Some(n) = self
            .syntax
            .children()
            .find(|n| n.kind() == SyntaxKind::TYPE_EXPR)
        {
            return Some(TypeExpr { syntax: n });
        }

        // With the new parser, parenthesized types have FUNCTION_TYPE_PARAM children
        // that wrap the inner TYPE_EXPR. If there's exactly one FUNCTION_TYPE_PARAM
        // (and no arrow, which we already checked above), get its inner type.
        let params: Vec<_> = self
            .syntax
            .children()
            .filter(|n| n.kind() == SyntaxKind::FUNCTION_TYPE_PARAM)
            .collect();

        if params.len() == 1 {
            // Get the TYPE_EXPR from inside the FUNCTION_TYPE_PARAM
            return params[0]
                .children()
                .find(|n| n.kind() == SyntaxKind::TYPE_EXPR)
                .map(|n| TypeExpr { syntax: n });
        }

        None
    }

    /// Get all child `TypeExpr` nodes.
    ///
    /// For union types where the parser creates child `TYPE_EXPR` for each member,
    /// this returns those members. Returns empty vec if no children.
    pub fn child_type_exprs(&self) -> Vec<TypeExpr> {
        self.syntax
            .children()
            .filter(|n| n.kind() == SyntaxKind::TYPE_EXPR)
            .map(|n| TypeExpr { syntax: n })
            .collect()
    }

    /// Get the `TYPE_ARGS` node for generic types like `map<K, V>`.
    pub fn type_args(&self) -> Option<SyntaxNode> {
        self.syntax
            .children()
            .find(|n| n.kind() == SyntaxKind::TYPE_ARGS)
    }

    /// Get the type argument `TypeExprs` from `TYPE_ARGS`.
    pub fn type_arg_exprs(&self) -> Vec<TypeExpr> {
        self.type_args()
            .map(|args| {
                args.children()
                    .filter(|n| n.kind() == SyntaxKind::TYPE_EXPR)
                    .map(|n| TypeExpr { syntax: n })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get the base type name (the first WORD token).
    ///
    /// For `int[]?` returns `Some("int")`.
    /// For `map<K, V>` returns `Some("map")`.
    /// For `"user"` returns `None` (it's a string literal, not a named type).
    pub fn base_name(&self) -> Option<String> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| t.kind() == SyntaxKind::WORD)
            .map(|t| t.text().to_string())
    }

    /// Get the full dotted type name (all WORD tokens joined by DOTs).
    ///
    /// For `baml.http.Request` returns `Some("baml.http.Request")`.
    /// For `int` returns `Some("int")`.
    /// For `"user"` returns `None`.
    pub fn dotted_name(&self) -> Option<String> {
        let tokens: Vec<_> = self
            .syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .collect();
        extract_dotted_name(tokens.iter())
    }

    /// Check if this is a string literal type like `"user"`.
    pub fn string_literal(&self) -> Option<String> {
        self.syntax
            .children()
            .find(|n| n.kind() == SyntaxKind::STRING_LITERAL)
            .map(|n| {
                // Get text and trim leading/trailing whitespace (trivia)
                let text = n.text().to_string();
                let trimmed = text.trim();
                // Well-formed: starts AND ends with quote
                if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
                    trimmed[1..trimmed.len() - 1].to_string()
                } else {
                    // Malformed (error recovery): preserve full text, just strip leading quote
                    trimmed.trim_start_matches('"').to_string()
                }
            })
    }

    /// Check if this is an integer literal type like `200`.
    pub fn integer_literal(&self) -> Option<i64> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| t.kind() == SyntaxKind::INTEGER_LITERAL)
            .and_then(|t| t.text().parse().ok())
    }

    /// Check if this is a boolean literal (`true` or `false`).
    pub fn bool_literal(&self) -> Option<bool> {
        let name = self.base_name()?;
        match name.as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        }
    }

    /// Get the parts of this type expression for each union member.
    ///
    /// Returns a list of `UnionMemberParts`, where each contains the tokens
    /// and child nodes for one union member. This allows parsing union members
    /// by token/node kinds instead of string manipulation.
    ///
    /// For `int | string[]`, returns two `UnionMemberParts`:
    /// - First: tokens=\[WORD("int")\]
    /// - Second: tokens=\[WORD("string"), `L_BRACKET`, `R_BRACKET`\]
    ///
    /// For `"user" | int`, returns two `UnionMemberParts`:
    /// - First: `child_nodes`=\[`STRING_LITERAL`\], tokens=\[\]
    /// - Second: tokens=\[WORD("int")\]
    pub fn union_member_parts(&self) -> Vec<UnionMemberParts> {
        let mut members = Vec::new();
        let mut current = UnionMemberParts::new();

        for child in self.syntax.children_with_tokens() {
            match child {
                rowan::NodeOrToken::Token(token) => {
                    if token.kind().is_trivia() {
                        continue;
                    }
                    if token.kind() == SyntaxKind::PIPE {
                        if !current.is_empty() {
                            members.push(current);
                            current = UnionMemberParts::new();
                        }
                    } else {
                        current.tokens.push(token);
                    }
                }
                rowan::NodeOrToken::Node(child_node) => {
                    current.child_nodes.push(child_node);
                }
            }
        }

        if !current.is_empty() {
            members.push(current);
        }

        members
    }

    /// Get the text range of this type expression.
    ///
    /// This is useful for error reporting and span creation.
    pub fn text_range(&self) -> rowan::TextRange {
        self.syntax.text_range()
    }

    /// Check if this is a function type: `(x: int, y: int) -> bool` or `(int) -> bool`.
    ///
    /// A function type has:
    /// - An `L_PAREN` token
    /// - Zero or more `FUNCTION_TYPE_PARAM` children
    /// - An `R_PAREN` token
    /// - An `ARROW` token
    /// - A return type `TYPE_EXPR`
    pub fn is_function_type(&self) -> bool {
        // Check for ARROW token at the top level (not inside nested TYPE_EXPR)
        // The arrow must be a direct child token, not inside a child node
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .any(|t| t.kind() == SyntaxKind::ARROW)
    }

    /// Get the parameters of a function type.
    ///
    /// Returns an empty vec if this is not a function type.
    /// Each parameter is wrapped in a `FunctionTypeParam` which provides
    /// access to the optional name and the type.
    pub fn function_type_params(&self) -> Vec<FunctionTypeParam> {
        self.syntax
            .children()
            .filter(|n| n.kind() == SyntaxKind::FUNCTION_TYPE_PARAM)
            .map(|n| FunctionTypeParam { syntax: n })
            .collect()
    }

    /// Get the return type of a function type.
    ///
    /// For `(x: int) -> string`, returns the `TypeExpr` for `string`.
    /// Returns None if this is not a function type or if the return type is missing.
    pub fn function_return_type(&self) -> Option<TypeExpr> {
        if !self.is_function_type() {
            return None;
        }
        // The return type is the TYPE_EXPR that comes after the ARROW
        // We need to find the TYPE_EXPR that is NOT inside a FUNCTION_TYPE_PARAM
        // Since FUNCTION_TYPE_PARAMs contain their own TYPE_EXPRs, we look for
        // the direct child TYPE_EXPR (which is the return type)
        self.syntax
            .children()
            .filter(|n| n.kind() == SyntaxKind::TYPE_EXPR)
            .map(|n| TypeExpr { syntax: n })
            .last() // The return type is typically the last TYPE_EXPR
    }
}

/// A parameter in a function type expression.
///
/// Can be either:
/// - Named: `x: int`
/// - Unnamed: `int`
///
/// Parameter names are for documentation only and do not affect type equality.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionTypeParam {
    syntax: SyntaxNode,
}

impl BamlAstNode for FunctionTypeParam {}

impl AstNode for FunctionTypeParam {
    type Language = crate::BamlLanguage;

    fn can_cast(kind: <Self::Language as rowan::Language>::Kind) -> bool {
        kind == SyntaxKind::FUNCTION_TYPE_PARAM
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

impl FunctionTypeParam {
    /// Get the parameter name if present.
    ///
    /// For `x: int`, returns `Some("x")`.
    /// For just `int`, returns `None`.
    pub fn name(&self) -> Option<String> {
        // If there's a COLON, the first WORD before it is the name
        let has_colon = self
            .syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .any(|t| t.kind() == SyntaxKind::COLON);

        if has_colon {
            self.syntax
                .children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .find(|t| t.kind() == SyntaxKind::WORD)
                .map(|t| t.text().to_string())
        } else {
            None
        }
    }

    /// Get the type of this parameter.
    ///
    /// For `x: int`, returns the `TypeExpr` for `int`.
    /// For just `int`, returns the `TypeExpr` for `int`.
    pub fn ty(&self) -> Option<TypeExpr> {
        self.syntax
            .children()
            .find(|n| n.kind() == SyntaxKind::TYPE_EXPR)
            .map(|n| TypeExpr { syntax: n })
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

impl TemplateStringDef {
    /// Get the template string name.
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| {
                token.kind() == SyntaxKind::WORD && token.parent() == Some(self.syntax.clone())
            })
    }

    /// Get the parameter list.
    pub fn param_list(&self) -> Option<ParameterList> {
        self.syntax.children().find_map(ParameterList::cast)
    }

    /// Get the raw string literal containing the template body.
    pub fn raw_string(&self) -> Option<RawStringLiteral> {
        self.syntax.children().find_map(RawStringLiteral::cast)
    }
}

impl LlmFunctionBody {
    /// Get the client field if present.
    ///
    /// For `function Foo() -> string { client GPT4 ... }`, returns the `client GPT4` field.
    pub fn client_field(&self) -> Option<ClientField> {
        self.syntax.children().find_map(ClientField::cast)
    }

    /// Get the prompt field if present.
    ///
    /// For `function Foo() -> string { ... prompt #"..."# }`, returns the `prompt #"..."#` field.
    pub fn prompt_field(&self) -> Option<PromptField> {
        self.syntax.children().find_map(PromptField::cast)
    }
}

impl ClientField {
    /// Get the client name token if it's a simple identifier.
    ///
    /// For `client GPT4`, returns the `GPT4` token.
    /// For `client "openai/gpt-4o"`, returns None (use `name_or_string()` instead).
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| token.kind() == SyntaxKind::WORD)
    }

    /// Get the client value as a string, whether it's an identifier or a string literal.
    ///
    /// For `client GPT4`, returns "GPT4".
    /// For `client "openai/gpt-4o"`, returns "openai/gpt-4o".
    pub fn value(&self) -> Option<String> {
        // First try to get it as a simple identifier (WORD token)
        if let Some(token) = self.name() {
            return Some(token.text().to_string());
        }

        // Otherwise, try to get it as a string literal
        if let Some(string_node) = self.syntax.children().find_map(StringLiteral::cast) {
            return Some(string_node.value());
        }

        None
    }
}

impl PromptField {
    /// Get the raw string literal node containing the prompt.
    ///
    /// For `prompt #"Hello {{ name }}"#`, returns the `#"Hello {{ name }}"#` node.
    pub fn raw_string(&self) -> Option<RawStringLiteral> {
        self.syntax.children().find_map(RawStringLiteral::cast)
    }
}

impl StringLiteral {
    /// Get the value of the string literal, without the surrounding quotes.
    ///
    /// For `"hello world"`, returns `hello world`.
    pub fn value(&self) -> String {
        let text = self.syntax.text().to_string();
        // String literals are of the form: "..." (tokens: Quote, words/spaces, Quote)
        // Strip leading and trailing quote characters
        if text.starts_with('"') && text.ends_with('"') && text.len() > 2 {
            text[1..text.len() - 1].to_string()
        } else {
            text
        }
    }
}

impl RawStringLiteral {
    /// Get the full text of the raw string literal, including delimiters.
    ///
    /// For `#"Hello"#`, returns `#"Hello"#`.
    pub fn full_text(&self) -> String {
        self.syntax.text().to_string()
    }

    /// Get all Jinja expressions in the raw string.
    ///
    /// For `#"Hello {{ name }}"#`, returns the `{{ name }}` node.
    pub fn jinja_expressions(&self) -> impl Iterator<Item = JinjaExpression> {
        self.syntax.children().filter_map(JinjaExpression::cast)
    }

    /// Get all Jinja statements in the raw string.
    ///
    /// For `#"{% if x %}...{% endif %}"#`, returns the `{% if x %}` and `{% endif %}` nodes.
    pub fn jinja_statements(&self) -> impl Iterator<Item = JinjaStatement> {
        self.syntax.children().filter_map(JinjaStatement::cast)
    }

    /// Get all Jinja comments in the raw string.
    ///
    /// For `#"{# comment #}"#`, returns the `{# comment #}` node.
    pub fn jinja_comments(&self) -> impl Iterator<Item = JinjaComment> {
        self.syntax.children().filter_map(JinjaComment::cast)
    }

    /// Get all prompt text nodes in the raw string.
    ///
    /// For `#"Hello {{ name }}"#`, returns the `Hello ` text node.
    pub fn prompt_texts(&self) -> impl Iterator<Item = PromptText> {
        self.syntax.children().filter_map(PromptText::cast)
    }
}

impl JinjaExpression {
    /// Get the inner text of the Jinja expression, without the {{ }} delimiters.
    ///
    /// For `{{ input.name }}`, returns `input.name` (with whitespace trimmed).
    pub fn inner_text(&self) -> String {
        let text = self.syntax.text().to_string();
        // Strip {{ and }}
        if text.starts_with("{{") && text.ends_with("}}") {
            text[2..text.len() - 2].trim().to_string()
        } else {
            text
        }
    }

    /// Get the full text of the Jinja expression, including {{ }} delimiters.
    pub fn full_text(&self) -> String {
        self.syntax.text().to_string()
    }
}

impl JinjaStatement {
    /// Get the inner text of the Jinja statement, without the {% %} delimiters.
    ///
    /// For `{% if condition %}`, returns `if condition` (with whitespace trimmed).
    pub fn inner_text(&self) -> String {
        let text = self.syntax.text().to_string();
        // Strip {% and %}
        if text.starts_with("{%") && text.ends_with("%}") {
            text[2..text.len() - 2].trim().to_string()
        } else {
            text
        }
    }

    /// Get the full text of the Jinja statement, including {% %} delimiters.
    pub fn full_text(&self) -> String {
        self.syntax.text().to_string()
    }
}

impl JinjaComment {
    /// Get the inner text of the Jinja comment, without the {# #} delimiters.
    ///
    /// For `{# this is a comment #}`, returns `this is a comment` (with whitespace trimmed).
    pub fn inner_text(&self) -> String {
        let text = self.syntax.text().to_string();
        // Strip {# and #}
        if text.starts_with("{#") && text.ends_with("#}") {
            text[2..text.len() - 2].trim().to_string()
        } else {
            text
        }
    }

    /// Get the full text of the Jinja comment, including {# #} delimiters.
    pub fn full_text(&self) -> String {
        self.syntax.text().to_string()
    }
}

impl PromptText {
    /// Get the text content.
    pub fn text(&self) -> String {
        self.syntax.text().to_string()
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

    /// Get all `type_builder` blocks inside this config block.
    pub fn type_builder_blocks(&self) -> impl Iterator<Item = TypeBuilderBlock> {
        self.syntax.children().filter_map(TypeBuilderBlock::cast)
    }
}

impl TypeBuilderBlock {
    /// Get the `type_builder` keyword token.
    pub fn keyword(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| token.kind() == SyntaxKind::KW_TYPE_BUILDER)
    }

    /// Get all class definitions (non-dynamic).
    pub fn classes(&self) -> impl Iterator<Item = ClassDef> {
        self.syntax
            .children()
            .filter(|n| n.kind() == SyntaxKind::CLASS_DEF)
            .filter_map(ClassDef::cast)
    }

    /// Get all enum definitions (non-dynamic).
    pub fn enums(&self) -> impl Iterator<Item = EnumDef> {
        self.syntax
            .children()
            .filter(|n| n.kind() == SyntaxKind::ENUM_DEF)
            .filter_map(EnumDef::cast)
    }

    /// Get all dynamic type definitions (dynamic class or dynamic enum).
    pub fn dynamic_types(&self) -> impl Iterator<Item = DynamicTypeDef> {
        self.syntax.children().filter_map(DynamicTypeDef::cast)
    }

    /// Get all type alias definitions.
    pub fn type_aliases(&self) -> impl Iterator<Item = TypeAliasDef> {
        self.syntax.children().filter_map(TypeAliasDef::cast)
    }
}

impl DynamicTypeDef {
    /// Get the class definition inside this dynamic type def (if it's a dynamic class).
    pub fn class(&self) -> Option<ClassDef> {
        self.syntax.children().find_map(ClassDef::cast)
    }

    /// Get the enum definition inside this dynamic type def (if it's a dynamic enum).
    pub fn enum_def(&self) -> Option<EnumDef> {
        self.syntax.children().find_map(EnumDef::cast)
    }

    /// Check if this is a dynamic class.
    pub fn is_class(&self) -> bool {
        self.class().is_some()
    }

    /// Check if this is a dynamic enum.
    pub fn is_enum(&self) -> bool {
        self.enum_def().is_some()
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

    /// Check if the value is an array literal.
    pub fn is_array(&self) -> bool {
        self.syntax
            .children()
            .find(|child| child.kind() == SyntaxKind::CONFIG_VALUE)
            .map(|config_value| {
                config_value
                    .children()
                    .any(|child| child.kind() == SyntaxKind::ARRAY_LITERAL)
            })
            .unwrap_or(false)
    }

    /// Get the `CONFIG_VALUE` syntax node if present.
    ///
    /// This gives access to the raw syntax tree for examining expression structure.
    pub fn config_value_node(&self) -> Option<SyntaxNode> {
        self.syntax
            .children()
            .find(|child| child.kind() == SyntaxKind::CONFIG_VALUE)
    }

    /// Get array elements, returning only those that are string literals.
    ///
    /// Returns `None` if this is not an array.
    /// For each element, returns `Some(string_value)` if it's a string literal,
    /// or `None` if it's some other type (number, identifier, etc.).
    /// The `TextRange` is always returned for error reporting on non-string elements.
    pub fn array_string_elements(&self) -> Option<Vec<(Option<String>, rowan::TextRange)>> {
        let config_value = self
            .syntax
            .children()
            .find(|child| child.kind() == SyntaxKind::CONFIG_VALUE)?;

        let array_literal = config_value
            .children()
            .find(|child| child.kind() == SyntaxKind::ARRAY_LITERAL)?;

        Some(
            array_literal
                .children()
                .filter(|child| child.kind() == SyntaxKind::CONFIG_VALUE)
                .map(|element| {
                    // Check if this element contains a string literal
                    let has_string_literal = element.descendants().any(|node| {
                        matches!(
                            node.kind(),
                            SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL
                        )
                    });

                    if has_string_literal {
                        // Extract the string content (excluding quotes)
                        let value: String = element
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
                                        | SyntaxKind::L_BRACKET
                                        | SyntaxKind::R_BRACKET
                                        | SyntaxKind::COMMA
                                )
                            })
                            .map(|token| token.text().to_string())
                            .collect();
                        (Some(value), element.text_range())
                    } else {
                        // Not a string literal - return None for the value
                        (None, element.text_range())
                    }
                })
                .collect(),
        )
    }

    /// Get the raw `SyntaxNode` for the array literal, if this value is an array.
    pub fn array_node(&self) -> Option<SyntaxNode> {
        let config_value = self
            .syntax
            .children()
            .find(|child| child.kind() == SyntaxKind::CONFIG_VALUE)?;

        config_value
            .children()
            .find(|child| child.kind() == SyntaxKind::ARRAY_LITERAL)
    }

    /// Check if this config item's key matches the given name.
    ///
    /// This is a convenience method to avoid the common pattern:
    /// `item.key().map(|k| k.text() == "name").unwrap_or(false)`
    ///
    /// # Example
    /// ```ignore
    /// // Instead of:
    /// block.items().find(|item| item.key().map(|k| k.text() == "provider").unwrap_or(false))
    ///
    /// // Use:
    /// block.items().find(|item| item.matches_key("provider"))
    /// ```
    pub fn matches_key(&self, name: &str) -> bool {
        self.key().is_some_and(|k| k.text() == name)
    }

    /// Get attributes attached to this config item (e.g., `args { ... } @check(...)`).
    pub fn attributes(&self) -> impl Iterator<Item = Attribute> {
        self.syntax.children().filter_map(Attribute::cast)
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
        // Look for a ConfigItem with key "functions" and extract all function names.
        // The function names are inside a CONFIG_VALUE child node, not in attributes.
        self.syntax
            .descendants()
            .filter_map(ConfigItem::cast)
            .find(|item| item.matches_key("functions"))
            .and_then(|item| {
                // Find the CONFIG_VALUE child (excludes attributes which are siblings)
                item.syntax()
                    .children()
                    .find(|child| child.kind() == SyntaxKind::CONFIG_VALUE)
            })
            .map(|config_value| {
                config_value
                    .descendants_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .filter(|token| token.kind() == SyntaxKind::WORD)
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

    /// Check if block attribute has arguments (parentheses with content).
    pub fn has_args(&self) -> bool {
        self.syntax
            .children()
            .any(|child| child.kind() == SyntaxKind::ATTRIBUTE_ARGS)
    }

    /// Get the text range of the argument node (for error reporting).
    pub fn args_span(&self) -> Option<rowan::TextRange> {
        self.syntax
            .children()
            .find(|child| child.kind() == SyntaxKind::ATTRIBUTE_ARGS)
            .map(|args| args.text_range())
    }

    /// Get the first string argument value (unquoted).
    /// Returns None if no `ATTRIBUTE_ARGS` or no string literal found.
    /// Preserves internal whitespace within the string.
    pub fn string_arg(&self) -> Option<String> {
        let args = self
            .syntax
            .children()
            .find(|child| child.kind() == SyntaxKind::ATTRIBUTE_ARGS)?;

        // First, try to find a STRING_LITERAL or RAW_STRING_LITERAL node and extract its content
        for child in args.children() {
            match child.kind() {
                SyntaxKind::STRING_LITERAL => {
                    // Get full text and strip quotes: "content" -> content
                    let text = child.text().to_string();
                    let trimmed = text.trim();
                    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
                        return Some(trimmed[1..trimmed.len() - 1].to_string());
                    }
                }
                SyntaxKind::RAW_STRING_LITERAL => {
                    // Get full text and strip raw string delimiters: #"content"# -> content
                    let text = child.text().to_string();
                    let trimmed = text.trim();
                    // Count leading hashes
                    let hash_count = trimmed.chars().take_while(|&c| c == '#').count();
                    if hash_count > 0 {
                        // Strip #..."..."#
                        let inner = &trimmed[hash_count..];
                        if inner.starts_with('"') {
                            if let Some(end_pos) = inner.rfind('"') {
                                if end_pos > 0 {
                                    return Some(inner[1..end_pos].to_string());
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        // Fallback: collect non-structural tokens (for unquoted strings)
        let result: String = args
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
                        | SyntaxKind::L_PAREN
                        | SyntaxKind::R_PAREN
                        | SyntaxKind::COMMA
                )
            })
            .map(|token| token.text().to_string())
            .collect();

        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    /// Check if the argument is a valid string literal (not an expression or identifier).
    pub fn arg_is_string_literal(&self) -> bool {
        let Some(args) = self
            .syntax
            .children()
            .find(|child| child.kind() == SyntaxKind::ATTRIBUTE_ARGS)
        else {
            return false;
        };

        args.descendants_with_tokens().any(|child| {
            matches!(
                child.kind(),
                SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL
            )
        })
    }

    /// Get all argument nodes in this attribute.
    ///
    /// Each argument is one of:
    /// - `STRING_LITERAL` for `"quoted"`
    /// - `RAW_STRING_LITERAL` for `#"raw"#`
    /// - `EXPR` for `{{ jinja }}`
    /// - `UNQUOTED_STRING` for bare words
    pub fn args(&self) -> impl Iterator<Item = SyntaxNode> + '_ {
        self.syntax
            .children()
            .find(|child| child.kind() == SyntaxKind::ATTRIBUTE_ARGS)
            .into_iter()
            .flat_map(|args| args.children())
            .filter(|child| {
                matches!(
                    child.kind(),
                    SyntaxKind::STRING_LITERAL
                        | SyntaxKind::RAW_STRING_LITERAL
                        | SyntaxKind::EXPR
                        | SyntaxKind::UNQUOTED_STRING
                )
            })
    }

    /// Count the number of arguments.
    pub fn arg_count(&self) -> usize {
        self.args().count()
    }

    /// Check if this attribute has exactly one argument that is a string literal.
    pub fn has_single_string_arg(&self) -> bool {
        self.arg_count() == 1 && self.arg_is_string_literal()
    }

    /// Check if the argument is a string literal or unquoted string (not an expression).
    pub fn arg_is_string_or_unquoted(&self) -> bool {
        let Some(args) = self
            .syntax
            .children()
            .find(|child| child.kind() == SyntaxKind::ATTRIBUTE_ARGS)
        else {
            return false;
        };

        args.descendants_with_tokens().any(|child| {
            matches!(
                child.kind(),
                SyntaxKind::STRING_LITERAL
                    | SyntaxKind::RAW_STRING_LITERAL
                    | SyntaxKind::UNQUOTED_STRING
            )
        })
    }

    /// Check if this attribute has exactly one argument that is a string literal or unquoted string.
    pub fn has_single_string_or_unquoted_arg(&self) -> bool {
        self.arg_count() == 1 && self.arg_is_string_or_unquoted()
    }
}

impl Attribute {
    /// Get the first segment of the attribute name (e.g., "stream" from @stream.done).
    /// Also handles keyword attribute names like @assert and @check.
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| matches!(token.kind(), SyntaxKind::WORD | SyntaxKind::KW_ASSERT))
    }

    /// Get the full attribute name including dot-separated modifiers.
    /// For @stream.done returns "stream.done", for @alias returns "alias".
    /// Also handles keyword attribute names like @assert and @check.
    pub fn full_name(&self) -> Option<String> {
        let segments: Vec<String> = self
            .syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| matches!(token.kind(), SyntaxKind::WORD | SyntaxKind::KW_ASSERT))
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
    /// Also handles keyword attribute names like @assert and @check.
    pub fn full_name_range(&self) -> Option<rowan::TextRange> {
        let tokens: Vec<_> = self
            .syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| {
                matches!(
                    token.kind(),
                    SyntaxKind::WORD | SyntaxKind::KW_ASSERT | SyntaxKind::DOT
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

    /// Check if attribute has arguments (parentheses with content).
    pub fn has_args(&self) -> bool {
        self.syntax
            .children()
            .any(|child| child.kind() == SyntaxKind::ATTRIBUTE_ARGS)
    }

    /// Get the text range of the argument node (for error reporting).
    pub fn args_span(&self) -> Option<rowan::TextRange> {
        self.syntax
            .children()
            .find(|child| child.kind() == SyntaxKind::ATTRIBUTE_ARGS)
            .map(|args| args.text_range())
    }

    /// Get the first string argument value (unquoted).
    /// Returns None if no `ATTRIBUTE_ARGS` or no string literal found.
    /// For @alias("foo") returns Some("foo").
    /// Preserves internal whitespace within the string.
    pub fn string_arg(&self) -> Option<String> {
        let args = self
            .syntax
            .children()
            .find(|child| child.kind() == SyntaxKind::ATTRIBUTE_ARGS)?;

        // First, try to find a STRING_LITERAL or RAW_STRING_LITERAL node and extract its content
        for child in args.children() {
            match child.kind() {
                SyntaxKind::STRING_LITERAL => {
                    // Get full text and strip quotes: "content" -> content
                    let text = child.text().to_string();
                    let trimmed = text.trim();
                    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
                        return Some(trimmed[1..trimmed.len() - 1].to_string());
                    }
                }
                SyntaxKind::RAW_STRING_LITERAL => {
                    // Get full text and strip raw string delimiters: #"content"# -> content
                    let text = child.text().to_string();
                    let trimmed = text.trim();
                    // Count leading hashes
                    let hash_count = trimmed.chars().take_while(|&c| c == '#').count();
                    if hash_count > 0 {
                        // Strip #..."..."#
                        let inner = &trimmed[hash_count..];
                        if inner.starts_with('"') {
                            if let Some(end_pos) = inner.rfind('"') {
                                if end_pos > 0 {
                                    return Some(inner[1..end_pos].to_string());
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        // Fallback: collect non-structural tokens (for unquoted strings)
        let result: String = args
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
                        | SyntaxKind::L_PAREN
                        | SyntaxKind::R_PAREN
                        | SyntaxKind::COMMA
                )
            })
            .map(|token| token.text().to_string())
            .collect();

        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    /// Check if the argument is a valid string literal (not an expression or identifier).
    /// Returns true if the argument contains `STRING_LITERAL` or `RAW_STRING_LITERAL`.
    pub fn arg_is_string_literal(&self) -> bool {
        let Some(args) = self
            .syntax
            .children()
            .find(|child| child.kind() == SyntaxKind::ATTRIBUTE_ARGS)
        else {
            return false;
        };

        // Check if we have a STRING_LITERAL or RAW_STRING_LITERAL node/token
        args.descendants_with_tokens().any(|child| {
            matches!(
                child.kind(),
                SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL
            )
        })
    }

    /// Get all argument nodes in this attribute.
    ///
    /// Each argument is one of:
    /// - `STRING_LITERAL` for `"quoted"`
    /// - `RAW_STRING_LITERAL` for `#"raw"#`
    /// - `EXPR` for `{{ jinja }}`
    /// - `UNQUOTED_STRING` for bare words
    pub fn args(&self) -> impl Iterator<Item = SyntaxNode> + '_ {
        self.syntax
            .children()
            .find(|child| child.kind() == SyntaxKind::ATTRIBUTE_ARGS)
            .into_iter()
            .flat_map(|args| args.children())
            .filter(|child| {
                matches!(
                    child.kind(),
                    SyntaxKind::STRING_LITERAL
                        | SyntaxKind::RAW_STRING_LITERAL
                        | SyntaxKind::EXPR
                        | SyntaxKind::UNQUOTED_STRING
                )
            })
    }

    /// Count the number of arguments.
    pub fn arg_count(&self) -> usize {
        self.args().count()
    }

    /// Check if this attribute has exactly one argument that is a string literal.
    pub fn has_single_string_arg(&self) -> bool {
        self.arg_count() == 1 && self.arg_is_string_literal()
    }

    /// Check if the argument is a string literal or unquoted string (not an expression).
    pub fn arg_is_string_or_unquoted(&self) -> bool {
        let Some(args) = self
            .syntax
            .children()
            .find(|child| child.kind() == SyntaxKind::ATTRIBUTE_ARGS)
        else {
            return false;
        };

        args.descendants_with_tokens().any(|child| {
            matches!(
                child.kind(),
                SyntaxKind::STRING_LITERAL
                    | SyntaxKind::RAW_STRING_LITERAL
                    | SyntaxKind::UNQUOTED_STRING
            )
        })
    }

    /// Check if this attribute has exactly one argument that is a string literal or unquoted string.
    pub fn has_single_string_or_unquoted_arg(&self) -> bool {
        self.arg_count() == 1 && self.arg_is_string_or_unquoted()
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

    /// Get the initializer as a token (for direct literals like integers, bools, null,
    /// or simple variable references).
    /// Returns the literal/identifier token if the initializer is a simple token.
    pub fn initializer_token(&self) -> Option<SyntaxToken> {
        // We need to find tokens AFTER the '=' sign, since the first WORD is the variable name
        let mut seen_equals = false;
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| {
                if token.kind() == SyntaxKind::EQUALS {
                    seen_equals = true;
                    return false;
                }
                // Only consider tokens after the '='
                if !seen_equals {
                    return false;
                }
                match token.kind() {
                    SyntaxKind::INTEGER_LITERAL
                    | SyntaxKind::FLOAT_LITERAL
                    | SyntaxKind::STRING_LITERAL
                    | SyntaxKind::RAW_STRING_LITERAL => true,
                    // WORD tokens can be boolean/null literals or variable references
                    SyntaxKind::WORD => true,
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
    ///
    /// For most statement nodes (`LET_STMT`, `BREAK_STMT`, etc.), the semicolon is a child of the node.
    /// For `WHILE_STMT` and `FOR_EXPR`, the semicolon is a sibling (parser doesn't consume it).
    /// For expression nodes and tokens, the semicolon is a sibling after the node.
    pub fn has_trailing_semicolon(&self) -> bool {
        use rowan::Direction;

        match self {
            BlockElement::Stmt(node) => {
                // WHILE_STMT and FOR_EXPR don't consume semicolons in the parser,
                // so check siblings like expressions
                if matches!(node.kind(), SyntaxKind::WHILE_STMT | SyntaxKind::FOR_EXPR) {
                    return node
                        .siblings_with_tokens(Direction::Next)
                        .skip(1)
                        .filter_map(rowan::NodeOrToken::into_token)
                        .any(|token| token.kind() == SyntaxKind::SEMICOLON);
                }
                // For other statements, semicolon is a CHILD of the node (parsed inside the statement)
                node.children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .any(|token| token.kind() == SyntaxKind::SEMICOLON)
            }
            BlockElement::ExprNode(node) => {
                // For expressions, semicolon is a SIBLING after the node
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
