//! Typed AST node wrappers for ergonomic tree access.

use rowan::ast::AstNode;

use crate::{SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};

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

    // Consume alternating DOT + WORD. `spawn`/`await` are reserved keywords
    // but valid as namespace segments after a `.` (e.g. `baml.spawn.SpawnParams`
    // in a type annotation), mirroring the parser's segment set.
    while let Some(t) = iter.next() {
        if t.kind() != SyntaxKind::DOT {
            break;
        }
        let Some(word) = iter.next() else { break };
        if !matches!(
            word.kind(),
            SyntaxKind::WORD | SyntaxKind::KW_SPAWN | SyntaxKind::KW_AWAIT
        ) {
            break;
        }
        parts.push(word.text().to_string());
    }

    Some(parts.join("."))
}

use baml_base::escape::{unescape_backtick_string_literal, unescape_string_literal};

/// Match an optional single leading `MINUS` followed by exactly one `target`
/// literal token, skipping trivia. Returns `(negated, token)` on a clean
/// match. Rejects `--42` (multiple signs), intervening non-trivia tokens, or
/// a missing literal — caller treats those as "not a signed literal." Used by
/// both `UnionMemberParts` and `TypeExpr` so the two paths agree on what
/// counts as a signed integer / float literal.
fn scan_signed_literal_token(
    tokens: impl IntoIterator<Item = SyntaxToken>,
    target: SyntaxKind,
) -> Option<(bool, SyntaxToken)> {
    let mut negated = false;
    let mut saw_minus = false;
    for tok in tokens {
        match tok.kind() {
            k if k.is_trivia() => continue,
            SyntaxKind::MINUS => {
                if saw_minus {
                    return None;
                }
                saw_minus = true;
                negated = true;
            }
            k if k == target => return Some((negated, tok)),
            _ => return None,
        }
    }
    None
}

fn decode_regular_string_literal_text(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        unescape_string_literal(&trimmed[1..trimmed.len() - 1])
    } else {
        trimmed.trim_start_matches('"').to_string()
    }
}

fn decode_raw_string_literal_text(text: &str) -> Option<String> {
    let trimmed = text.trim();
    let hash_count = trimmed.chars().take_while(|&c| c == '#').count();
    if hash_count == 0 {
        return None;
    }

    let rest = &trimmed[hash_count..];
    let closing = format!("\"{}", &trimmed[..hash_count]);
    if rest.len() < hash_count + 2 || !rest.starts_with('"') || !rest.ends_with(&closing) {
        return None;
    }

    Some(rest[1..rest.len() - 1 - hash_count].to_string())
}

fn attribute_args_node(syntax: &SyntaxNode) -> Option<SyntaxNode> {
    syntax
        .children()
        .find(|child| child.kind() == SyntaxKind::ATTRIBUTE_ARGS)
}

fn attribute_args_contain(syntax: &SyntaxNode, kinds: &[SyntaxKind]) -> bool {
    attribute_args_node(syntax).is_some_and(|args| {
        args.descendants_with_tokens()
            .any(|child| kinds.contains(&child.kind()))
    })
}

fn attribute_arg_is_string_literal(syntax: &SyntaxNode) -> bool {
    attribute_args_contain(
        syntax,
        &[SyntaxKind::STRING_LITERAL, SyntaxKind::RAW_STRING_LITERAL],
    )
}

fn attribute_arg_is_string_or_unquoted(syntax: &SyntaxNode) -> bool {
    attribute_args_contain(
        syntax,
        &[
            SyntaxKind::STRING_LITERAL,
            SyntaxKind::RAW_STRING_LITERAL,
            SyntaxKind::UNQUOTED_STRING,
        ],
    )
}

fn attribute_args(syntax: &SyntaxNode) -> impl Iterator<Item = SyntaxNode> + '_ {
    attribute_args_node(syntax)
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
ast_node!(InterfaceDef, INTERFACE_DEF);
ast_node!(ImplementsBlock, IMPLEMENTS_BLOCK);
ast_node!(ImplementsTarget, IMPLEMENTS_TARGET);
ast_node!(InterfaceFieldLink, INTERFACE_FIELD_LINK);
ast_node!(ImplementsFor, IMPLEMENTS_FOR);
ast_node!(ImplementsForTarget, IMPLEMENTS_FOR_TARGET);
ast_node!(RequiresClause, REQUIRES_CLAUSE);
ast_node!(MethodSig, METHOD_SIG);
ast_node!(AssociatedTypeDecl, ASSOCIATED_TYPE_DECL);
ast_node!(ClientDef, CLIENT_DEF);
ast_node!(TestDef, TEST_DEF);
ast_node!(RetryPolicyDef, RETRY_POLICY_DEF);
ast_node!(TemplateStringDef, TEMPLATE_STRING_DEF);
ast_node!(TypeAliasDef, TYPE_ALIAS_DEF);

ast_node!(ParameterList, PARAMETER_LIST);
ast_node!(Parameter, PARAMETER);
ast_node!(CallArg, CALL_ARG);
ast_node!(FunctionBody, FUNCTION_BODY);
ast_node!(LlmFunctionBody, LLM_FUNCTION_BODY);
ast_node!(ExprFunctionBody, EXPR_FUNCTION_BODY);
ast_node!(Field, FIELD);
ast_node!(EnumVariant, ENUM_VARIANT);
ast_node!(ConfigBlock, CONFIG_BLOCK);
ast_node!(ConfigItem, CONFIG_ITEM);
ast_node!(ConfigValue, CONFIG_VALUE);
ast_node!(ClientField, CLIENT_FIELD);
ast_node!(PromptField, PROMPT_FIELD);
ast_node!(RawStringLiteral, RAW_STRING_LITERAL);
ast_node!(StringLiteral, STRING_LITERAL);
ast_node!(BacktickStringLiteral, BACKTICK_STRING_LITERAL);
ast_node!(BacktickText, BACKTICK_TEXT);
ast_node!(BacktickInterpolation, BACKTICK_INTERPOLATION);

// Jinja template components (inside raw strings)
ast_node!(JinjaExpression, TEMPLATE_INTERPOLATION);
ast_node!(JinjaStatement, TEMPLATE_CONTROL);
ast_node!(JinjaComment, TEMPLATE_COMMENT);
ast_node!(PromptText, PROMPT_TEXT);

ast_node!(TypeExpr, TYPE_EXPR);
ast_node!(Attribute, ATTRIBUTE);
ast_node!(TypeBuilderBlock, TYPE_BUILDER_BLOCK);
ast_node!(DynamicTypeDef, DYNAMIC_TYPE_DEF);
ast_node!(ObjectField, OBJECT_FIELD);
ast_node!(GenericParam, GENERIC_PARAM);

impl CallArg {
    /// The name of a named argument `name = value` — a leading `WORD` (or
    /// `client`) immediately followed by `=`. `None` for a positional argument
    /// (whose first element is the value expression, not a name token).
    pub fn name(&self) -> Option<SyntaxToken> {
        let mut elements = self
            .syntax
            .children_with_tokens()
            .filter(|element| !element.kind().is_trivia());
        let first = elements.next()?.into_token()?;
        if !matches!(first.kind(), SyntaxKind::WORD | SyntaxKind::KW_CLIENT) {
            return None;
        }
        (elements.next()?.kind() == SyntaxKind::EQUALS).then_some(first)
    }
}

impl ObjectField {
    /// The bare-word key of `key: value` (or shorthand `key`).
    ///
    /// `None` when the key is a string literal (`"key": value`): such a key is a
    /// child node, not a bare word.
    pub fn key(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .find(|element| !element.kind().is_trivia())
            .and_then(rowan::NodeOrToken::into_token)
            .filter(|token| matches!(token.kind(), SyntaxKind::WORD | SyntaxKind::KW_CLIENT))
    }
}

impl GenericParam {
    /// The declared parameter name (`T` in `<T: Bound>`).
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| token.kind() == SyntaxKind::WORD)
    }
}

/// Parts of a union member for token-based parsing.
///
/// Union members can contain both tokens (WORD, `L_BRACKET`, etc.) and child nodes
/// (`STRING_LITERAL`, `TYPE_EXPR` for parenthesized types, `TYPE_ARGS` for generics).
#[derive(Debug, Clone)]
pub struct UnionMemberParts {
    /// Tokens in this union member (WORD, `L_BRACKET`, `R_BRACKET`, QUESTION, etc.).
    /// Trivia tokens should not be included.
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

    /// Source range of this member's type name — the leading run of `WORD`/`DOT`
    /// tokens (e.g. `long_word_123.foobar`), excluding postfix `[]`/`?` modifiers
    /// and generic args. Lets diagnostics like "unresolved type" point at the
    /// offending identifier rather than the whole union/compound type.
    ///
    /// Falls back to the full extent of the member's tokens and child nodes when
    /// there is no leading name (e.g. a string-literal member), and to `None`
    /// when the member is empty.
    pub fn span(&self) -> Option<rowan::TextRange> {
        let name: Vec<_> = self
            .tokens
            .iter()
            .take_while(|t| matches!(t.kind(), SyntaxKind::WORD | SyntaxKind::DOT))
            .collect();
        if let (Some(first), Some(last)) = (name.first(), name.last()) {
            return Some(rowan::TextRange::new(
                first.text_range().start(),
                last.text_range().end(),
            ));
        }
        let ranges = || {
            self.tokens
                .iter()
                .map(rowan::SyntaxToken::text_range)
                .chain(self.child_nodes.iter().map(rowan::SyntaxNode::text_range))
        };
        let start = ranges().map(rowan::TextRange::start).min()?;
        let end = ranges().map(rowan::TextRange::end).max()?;
        Some(rowan::TextRange::new(start, end))
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

    /// Get the postfix modifiers (`[]` and `?`) in application order (innermost first).
    ///
    /// Works like `TypeExpr::postfix_modifiers()` but operates on the token list
    /// of a union member instead of directly on CST children.
    ///
    /// For `Union???` returns `[Optional, Optional, Optional]`.
    /// For `Union[]??` returns `[Array, Optional, Optional]`.
    /// For `Union?[]?` returns `[Optional, Array, Optional]`.
    pub fn postfix_modifiers(&self) -> Vec<TypePostFixModifier> {
        collect_postfix_modifiers(self.tokens.iter().map(SyntaxToken::kind))
    }

    /// Count the number of `[]` array modifiers on this union member.
    ///
    /// For `int` returns 0, for `int[]` returns 1, for `int[][]` returns 2.
    pub fn array_depth(&self) -> usize {
        self.postfix_modifiers()
            .iter()
            .filter(|m| **m == TypePostFixModifier::Array)
            .count()
    }

    /// Check if this union member has a trailing `?` (optional modifier).
    pub fn is_optional(&self) -> bool {
        self.postfix_modifiers()
            .contains(&TypePostFixModifier::Optional)
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
            .map(|n| decode_regular_string_literal_text(&n.text().to_string()))
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

    /// Return `(base, interface, member)` for `(Base as Interface).Member`.
    pub fn associated_type_projection(&self) -> Option<(TypeExpr, TypeExpr, SyntaxToken)> {
        let mut child_types = self
            .child_nodes
            .iter()
            .filter(|n| n.kind() == SyntaxKind::TYPE_EXPR)
            .cloned()
            .map(|syntax| TypeExpr { syntax });
        let base = child_types.next()?;
        let interface = child_types.next()?;

        if self
            .tokens
            .first()
            .is_none_or(|t| t.kind() != SyntaxKind::L_PAREN)
        {
            return None;
        }
        if !self.tokens.iter().any(|t| t.kind() == SyntaxKind::KW_AS) {
            return None;
        }
        let dot_idx = self
            .tokens
            .iter()
            .rposition(|t| t.kind() == SyntaxKind::DOT)?;
        let member = self.tokens.get(dot_idx + 1)?;
        (member.kind() == SyntaxKind::WORD).then(|| (base, interface, member.clone()))
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

    /// Check if this member has a `BIGINT_LITERAL` token, optionally preceded
    /// by a single `MINUS` token (for negative literals like `-7n`). Returns
    /// `(negated, token)`; the token text still carries the trailing `n`.
    /// Value parsing happens in `baml_compiler2_ast` where diagnostics can be
    /// emitted.
    pub fn bigint_literal(&self) -> Option<(bool, SyntaxToken)> {
        scan_signed_literal_token(self.tokens.iter().cloned(), SyntaxKind::BIGINT_LITERAL)
    }

    /// Check if this member has an `INTEGER_LITERAL` token, optionally
    /// preceded by a single `MINUS` token (for negative literals like `-42`).
    /// Rejects `--42` and any other shape. Returns `(negated, token)`.
    pub fn integer_literal(&self) -> Option<(bool, SyntaxToken)> {
        scan_signed_literal_token(self.tokens.iter().cloned(), SyntaxKind::INTEGER_LITERAL)
    }

    /// Check if this member has a `FLOAT_LITERAL` token. A single leading
    /// `MINUS` negates. Returns `(negated, token)`.
    pub fn float_literal(&self) -> Option<(bool, SyntaxToken)> {
        scan_signed_literal_token(self.tokens.iter().cloned(), SyntaxKind::FLOAT_LITERAL)
    }

    /// Get ATTRIBUTE child nodes from this union member.
    pub fn attributes(&self) -> impl Iterator<Item = Attribute> + '_ {
        self.child_nodes
            .iter()
            .filter_map(|n| Attribute::cast(n.clone()))
    }
}

impl Default for UnionMemberParts {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypePostFixModifier {
    Optional,
    Array,
}

/// Shared helper: scan a stream of `SyntaxKind`s and collect postfix modifiers
/// (`?` → Optional, `[]` → Array) in application order (innermost first).
/// Assumes that trivia tokens are not included.
fn collect_postfix_modifiers(kinds: impl Iterator<Item = SyntaxKind>) -> Vec<TypePostFixModifier> {
    let mut mods = Vec::new();
    let mut last = None;
    for kind in kinds {
        match kind {
            SyntaxKind::QUESTION => mods.push(TypePostFixModifier::Optional),
            SyntaxKind::R_BRACKET if last == Some(SyntaxKind::L_BRACKET) => {
                mods.push(TypePostFixModifier::Array);
            }
            _ => (),
        }
        last = Some(kind);
    }

    mods
}

impl TypeExpr {
    /// Return `(base, interface, member)` for `(Base as Interface).Member`.
    pub fn associated_type_projection(&self) -> Option<(TypeExpr, TypeExpr, SyntaxToken)> {
        let mut child_types = self.syntax.children().filter_map(TypeExpr::cast);
        let base = child_types.next()?;
        let interface = child_types.next()?;

        let tokens: Vec<_> = self
            .syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|t| !t.kind().is_trivia())
            .collect();
        if tokens
            .first()
            .is_none_or(|t| t.kind() != SyntaxKind::L_PAREN)
        {
            return None;
        }
        if !tokens.iter().any(|t| t.kind() == SyntaxKind::KW_AS) {
            return None;
        }
        let dot_idx = tokens.iter().rposition(|t| t.kind() == SyntaxKind::DOT)?;
        let member = tokens.get(dot_idx + 1)?;
        (member.kind() == SyntaxKind::WORD).then(|| (base, interface, member.clone()))
    }

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

    /// Get the postfix modifiers (`[]` and `?`) in application order (innermost first).
    ///
    /// For `int` returns `[]`.
    /// For `int[]` returns `[Array]`.
    /// For `int[]?` returns `[Array, Optional]`.
    /// For `int[][]` returns `[Array, Array]`.
    /// For `int[][]?` returns `[Array, Array, Optional]`.
    /// For `int?[]` returns `[Optional, Array]`.
    pub fn postfix_modifiers(&self) -> Vec<TypePostFixModifier> {
        // if it's a union without parens, the postfix modifiers we'd find
        // are actually the modifiers on the final union member
        if self.is_union() {
            return Vec::new();
        }

        collect_postfix_modifiers(
            self.syntax
                .children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .filter(|t| !t.kind().is_trivia())
                .map(|t| t.kind()),
        )
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

    /// Get named associated type bindings from `TYPE_ARGS`, e.g. `Item = int`.
    pub fn type_arg_associated_bindings(&self) -> Vec<AssociatedTypeDecl> {
        self.type_args()
            .map(|args| {
                args.children()
                    .filter_map(AssociatedTypeDecl::cast)
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
            .map(|n| decode_regular_string_literal_text(&n.text().to_string()))
    }

    /// Check if this is a bigint literal type like `42n` or `-7n`. A single
    /// leading `MINUS` negates. Returns `(negated, token)`; the token text
    /// still carries the trailing `n`. Value parsing happens in
    /// `baml_compiler2_ast` where diagnostics can be emitted.
    pub fn bigint_literal(&self) -> Option<(bool, SyntaxToken)> {
        let tokens = self
            .syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token);
        scan_signed_literal_token(tokens, SyntaxKind::BIGINT_LITERAL)
    }

    /// Check if this is an integer literal type like `200` or `-42`. A
    /// single leading `MINUS` negates; `--42` and other shapes return `None`.
    /// Returns `(negated, token)`.
    pub fn integer_literal(&self) -> Option<(bool, SyntaxToken)> {
        let tokens = self
            .syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token);
        scan_signed_literal_token(tokens, SyntaxKind::INTEGER_LITERAL)
    }

    /// Check if this is a float literal type like `3.14` or `-3.14`. A
    /// single leading `MINUS` negates. Returns `(negated, token)`.
    pub fn float_literal(&self) -> Option<(bool, SyntaxToken)> {
        let tokens = self
            .syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token);
        scan_signed_literal_token(tokens, SyntaxKind::FLOAT_LITERAL)
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

    /// Get the throws clause of a function type, if present.
    pub fn function_throws_clause(&self) -> Option<ThrowsClause> {
        if !self.is_function_type() {
            return None;
        }
        self.syntax.children().find_map(ThrowsClause::cast)
    }

    /// Get the throws type of a function type, if present.
    pub fn function_throws_type(&self) -> Option<TypeExpr> {
        self.function_throws_clause()
            .and_then(|clause| clause.type_expr())
    }

    /// Get all attributes attached to this type expression.
    ///
    /// These are ATTRIBUTE nodes that are direct children of the `TYPE_EXPR` node.
    /// The parser creates these for type-level annotations like `@stream.done`.
    pub fn attributes(&self) -> impl Iterator<Item = Attribute> {
        self.syntax.children().filter_map(Attribute::cast)
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
                .find(|t| t.kind() == SyntaxKind::WORD || t.kind() == SyntaxKind::KW_CLIENT)
                .map(|t| t.text().to_string())
        } else {
            None
        }
    }

    /// Whether this parameter uses function-type optional syntax: `name?: T`.
    pub fn is_optional(&self) -> bool {
        let mut tokens = self
            .syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| !token.kind().is_trivia());

        matches!(
            (tokens.next(), tokens.next(), tokens.next()),
            (Some(first), Some(second), Some(third))
                if (first.kind() == SyntaxKind::WORD || first.kind() == SyntaxKind::KW_CLIENT)
                    && second.kind() == SyntaxKind::QUESTION
                    && third.kind() == SyntaxKind::COLON
        )
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LetStmt {
    syntax: SyntaxNode,
}

impl BamlAstNode for LetStmt {}

impl AstNode for LetStmt {
    type Language = crate::BamlLanguage;

    fn can_cast(kind: <Self::Language as rowan::Language>::Kind) -> bool {
        kind == SyntaxKind::LET_STMT
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
ast_node!(WhileLetStmt, WHILE_LET_STMT);
ast_node!(ForExpr, FOR_EXPR);
ast_node!(BlockExpr, BLOCK_EXPR);
ast_node!(ReturnStmt, RETURN_STMT);
ast_node!(ThrowStmt, THROW_STMT);
ast_node!(BreakStmt, BREAK_STMT);
ast_node!(ContinueStmt, CONTINUE_STMT);
ast_node!(DeferStmt, DEFER_STMT);
ast_node!(PathExpr, PATH_EXPR);
ast_node!(FieldAccessExpr, FIELD_ACCESS_EXPR);
ast_node!(UpcastExpr, UPCAST_EXPR);
ast_node!(EnvAccessExpr, ENV_ACCESS_EXPR);
ast_node!(MatchExpr, MATCH_EXPR);
ast_node!(MatchArm, MATCH_ARM);
ast_node!(MatchPattern, MATCH_PATTERN);
ast_node!(MatchGuard, MATCH_GUARD);
ast_node!(CatchExpr, CATCH_EXPR);
ast_node!(CatchClause, CATCH_CLAUSE);
ast_node!(CatchArm, CATCH_ARM);
ast_node!(CatchPattern, CATCH_PATTERN);
ast_node!(ThrowExpr, THROW_EXPR);
ast_node!(ReturnExpr, RETURN_EXPR);
ast_node!(ThrowsClause, THROWS_CLAUSE);

// Implement accessor methods
impl SourceFile {
    /// Iterate over all top-level items in the file.
    pub fn items(&self) -> impl Iterator<Item = Item> {
        self.syntax.children().filter_map(Item::cast)
    }
}

impl FunctionDef {
    /// Get the function name.
    ///
    /// Accepts BEP-044 keyword tokens (`implements`, `extends`, `interface`)
    /// in addition to plain WORD tokens — the parser admits them as method
    /// names so reflection helpers like `TypeValue.implements(...)` parse
    /// without renaming.
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| {
                let kind = token.kind();
                let is_name = kind == SyntaxKind::WORD
                    || kind == SyntaxKind::KW_IMPLEMENTS
                    || kind == SyntaxKind::KW_IMPLEMENT
                    || kind == SyntaxKind::KW_EXTENDS
                    || kind == SyntaxKind::KW_REQUIRES
                    || kind == SyntaxKind::KW_INTERFACE;
                is_name && token.parent() == Some(self.syntax.clone())
            })
            .nth(0)
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

    /// Get the throws clause if present (BEP-007).
    pub fn throws_clause(&self) -> Option<ThrowsClause> {
        self.syntax.children().find_map(ThrowsClause::cast)
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
    /// For `prompt #"Hello {{ name }}"#`, returns the `#"Hello {{ name }}"#` node
    /// (the legacy Jinja form). Returns `None` for a new-mode backtick prompt.
    pub fn raw_string(&self) -> Option<RawStringLiteral> {
        self.syntax.children().find_map(RawStringLiteral::cast)
    }

    /// Get the backtick string literal node containing a new-mode prompt.
    ///
    /// For `` prompt `Hello ${name}` ``, returns the `` `Hello ${name}` `` node.
    /// BEP-049 (M5f): a backtick prompt compiles to a prompt-tag closure instead
    /// of a stored Jinja template. Returns `None` for a `#"..."#` prompt.
    pub fn backtick_string(&self) -> Option<BacktickStringLiteral> {
        self.syntax.children().find_map(BacktickStringLiteral::cast)
    }
}

impl StringLiteral {
    /// Get the value of the string literal, without the surrounding quotes.
    ///
    /// For `"hello world"`, returns `hello world`.
    pub fn value(&self) -> String {
        let text = self.syntax.text().to_string();
        decode_regular_string_literal_text(&text)
    }
}

impl BacktickStringLiteral {
    /// Get the full text including the surrounding backtick runs.
    pub fn full_text(&self) -> String {
        self.syntax.text().to_string()
    }

    /// Number of backticks in the opening (and closing) delimiter.
    ///
    /// Returns 0 if the syntax is malformed (no opening backtick).
    pub fn delimiter_count(&self) -> usize {
        // Walk children_with_tokens and break on the first non-BACKTICK
        // element — whether it's a token (content) or a node (e.g.
        // BACKTICK_INTERPOLATION). `filter_map(into_token).take_while(...)`
        // would skip past nodes silently, causing the *closing* backticks
        // to be miscounted as part of the opener for inputs like
        // `` `${a}${b}` `` (no text between two interpolations).
        let mut count = 0;
        for el in self.syntax.children_with_tokens() {
            match el {
                rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::BACKTICK => {
                    count += 1;
                }
                _ => break,
            }
        }
        count
    }

    /// Inner content between the opening and closing backtick runs, before any
    /// escape decoding or dedenting. Returns `None` if the node is malformed
    /// (missing opening or closing run).
    pub fn raw_inner(&self) -> Option<String> {
        let text = self.syntax.text().to_string();
        let n = self.delimiter_count();
        if n == 0 || text.len() < 2 * n {
            return None;
        }
        // Strip N leading + N trailing backticks. Closing presence is enforced
        // by the parser (anchored-close rule).
        let opener_end = n;
        let closer_start = text.len().saturating_sub(n);
        if opener_end > closer_start {
            return None;
        }
        Some(text[opener_end..closer_start].to_string())
    }

    /// Whether the literal spans multiple source lines (and therefore qualifies
    /// for auto-dedent under §12).
    pub fn is_multiline(&self) -> bool {
        self.raw_inner().map(|s| s.contains('\n')).unwrap_or(false)
    }

    /// The decoded value of the backtick string, with escapes resolved and (if
    /// multi-line) dedented per `baml_base::dedent::preprocess_template`.
    ///
    /// **Treats `${...}` sequences as literal text** — this is the pre-interp
    /// view, preserved as a fallback for callers that haven't migrated to
    /// [`Self::segments`]. New code should prefer `segments()` so interpolated
    /// expressions are surfaced as host AST nodes.
    pub fn value(&self) -> String {
        let Some(inner) = self.raw_inner() else {
            return String::new();
        };
        let decoded = unescape_backtick_string_literal(&inner);
        if decoded.contains('\n') {
            baml_base::dedent::preprocess_template(&decoded)
        } else {
            decoded
        }
    }

    /// Split the literal into the alternating text and interpolation segments
    /// that downstream lowerers consume.
    ///
    /// For `` `Hello, ${user.name}!` `` returns:
    /// `[Text("Hello, "), Interp(<${user.name}>), Text("!")]`.
    ///
    /// Text segments are escape-decoded; multi-line content is dedented per
    /// BEP §12 (interpolations do not affect the min-indent calculation per
    /// §12 rule 8 — "Whitespace inside `${...}` is preserved verbatim").
    pub fn segments(&self) -> Vec<BacktickSegment> {
        build_segment_tree(&mut self.flat_parts().into_iter().peekable())
    }

    /// Like [`segments`](Self::segments), but also returns structural
    /// diagnostics for unclosed / mismatched / stray `${for}`/`${if}` block
    /// tags. Lowering uses this so a malformed template is reported instead of
    /// silently miscompiling.
    pub fn segments_with_errors(&self) -> (Vec<BacktickSegment>, Vec<BacktickStructuralError>) {
        let parts = self.flat_parts();
        let errors = validate_block_structure(&parts);
        let segs = build_segment_tree(&mut parts.into_iter().peekable());
        (segs, errors)
    }

    /// Pass (1) of the two-pass build: walk the CST into a flat `FlatPart`
    /// stream (text + interp + block-tag opens/closes) with whole-literal
    /// dedent and §13 whitespace control. Pass (2) — lifting matched
    /// open/close pairs into nested For / If segments — is `build_segment_tree`.
    fn flat_parts(&self) -> Vec<FlatPart> {
        let n = self.delimiter_count();
        if n == 0 {
            return Vec::new();
        }

        let elements: Vec<_> = self.syntax.children_with_tokens().collect();
        let total_backticks = elements
            .iter()
            .filter(|el| el.kind() == SyntaxKind::BACKTICK)
            .count();
        if total_backticks < 2 * n {
            return Vec::new();
        }
        let closing_start_index = total_backticks - n;
        let mut parts: Vec<FlatPart> = Vec::new();
        let mut current_text = String::new();
        let mut bt_seen = 0usize;

        let flush_text = |current_text: &mut String, parts: &mut Vec<FlatPart>| {
            if !current_text.is_empty() {
                parts.push(FlatPart::Text(std::mem::take(current_text)));
            }
        };

        for el in &elements {
            match el {
                rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::BACKTICK => {
                    if bt_seen >= n && bt_seen < closing_start_index {
                        current_text.push('`');
                    }
                    bt_seen += 1;
                }
                rowan::NodeOrToken::Token(t) => {
                    if bt_seen >= n && bt_seen <= closing_start_index {
                        current_text.push_str(t.text());
                    }
                }
                rowan::NodeOrToken::Node(child) => match child.kind() {
                    SyntaxKind::BACKTICK_INTERPOLATION => {
                        flush_text(&mut current_text, &mut parts);
                        parts.push(FlatPart::Interp(child.clone()));
                    }
                    SyntaxKind::BACKTICK_FOR_OPEN => {
                        flush_text(&mut current_text, &mut parts);
                        parts.push(FlatPart::ForOpen(child.clone()));
                    }
                    SyntaxKind::BACKTICK_ENDFOR => {
                        flush_text(&mut current_text, &mut parts);
                        parts.push(FlatPart::Endfor(child.clone()));
                    }
                    SyntaxKind::BACKTICK_IF_OPEN => {
                        flush_text(&mut current_text, &mut parts);
                        parts.push(FlatPart::IfOpen(child.clone()));
                    }
                    SyntaxKind::BACKTICK_ELSE_IF => {
                        flush_text(&mut current_text, &mut parts);
                        parts.push(FlatPart::ElseIf(child.clone()));
                    }
                    SyntaxKind::BACKTICK_ELSE => {
                        flush_text(&mut current_text, &mut parts);
                        parts.push(FlatPart::Else(child.clone()));
                    }
                    SyntaxKind::BACKTICK_ENDIF => {
                        flush_text(&mut current_text, &mut parts);
                        parts.push(FlatPart::Endif(child.clone()));
                    }
                    _ => {
                        if bt_seen >= n && bt_seen <= closing_start_index {
                            current_text.push_str(&child.text().to_string());
                        }
                    }
                },
            }
        }
        if !current_text.is_empty() {
            parts.push(FlatPart::Text(current_text));
        }

        // Decode escapes per text chunk.
        for p in &mut parts {
            if let FlatPart::Text(s) = p {
                *s = unescape_backtick_string_literal(s);
            }
        }

        // Dedent across the whole literal if any text chunk contains a
        // newline. Replace each non-text part with a single-char placeholder
        // so they don't influence min-indent, then split the dedented
        // result back into text segments and reattach the parts in order.
        let needs_dedent = parts
            .iter()
            .any(|p| matches!(p, FlatPart::Text(s) if s.contains('\n')));
        if needs_dedent {
            // Pick a placeholder that doesn't appear in user content
            // (ultrareview bug_006). Walk the PUA range U+E000..U+F8FF and
            // use the first codepoint not present in any text chunk.
            let content_chars: String = parts
                .iter()
                .filter_map(|p| match p {
                    FlatPart::Text(s) => Some(s.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("");
            let placeholder: char = (0xE000u32..=0xF8FFu32)
                .filter_map(char::from_u32)
                .find(|c| !content_chars.contains(*c))
                .unwrap_or('\u{F8FF}');

            let mut joined = String::new();
            let mut non_text_indices: Vec<usize> = Vec::new();
            for (i, p) in parts.iter().enumerate() {
                match p {
                    FlatPart::Text(s) => joined.push_str(s),
                    _ => {
                        non_text_indices.push(i);
                        joined.push(placeholder);
                    }
                }
            }
            let dedented = baml_base::dedent::preprocess_template(&joined);
            let pieces: Vec<&str> = dedented.split(placeholder).collect();

            // Rebuild `parts` in dedented order: one text piece per gap,
            // non-text parts in their original order.
            let mut rebuilt: Vec<FlatPart> =
                Vec::with_capacity(pieces.len() + non_text_indices.len());
            let mut non_text_iter = non_text_indices.into_iter();
            for (i, piece) in pieces.iter().enumerate() {
                if !piece.is_empty() {
                    rebuilt.push(FlatPart::Text((*piece).to_string()));
                }
                if i + 1 < pieces.len() {
                    if let Some(orig_idx) = non_text_iter.next() {
                        // Move the original non-text part into the rebuilt
                        // list. We need ownership; replace with a sentinel
                        // empty Text and ignore it in the iteration here.
                        let part =
                            std::mem::replace(&mut parts[orig_idx], FlatPart::Text(String::new()));
                        rebuilt.push(part);
                    }
                }
            }
            parts = rebuilt;
        }

        // BEP §13 whitespace control: a block tag that's "alone on its
        // line" (preceded only by ws back to a newline, followed only by
        // ws up to the next newline) consumes that surrounding ws and the
        // trailing newline. Inline `${expr}` interpolations and mid-line
        // block tags consume nothing. Applied to the flat sequence before
        // hierarchical lifting so it works uniformly across nested blocks.
        apply_block_tag_whitespace_rule(&mut parts);
        parts
    }
}

/// BEP §13: trim surrounding whitespace + trailing newline around block
/// tags (`${for}`, `${if}`, `${else}`, `${else if}`, `${endfor}`, `${endif}`)
/// that are alone on their source line. Inline `${expr}` interpolations
/// are explicitly excluded.
fn apply_block_tag_whitespace_rule(parts: &mut Vec<FlatPart>) {
    fn is_block_tag(p: &FlatPart) -> bool {
        matches!(
            p,
            FlatPart::ForOpen(_)
                | FlatPart::IfOpen(_)
                | FlatPart::ElseIf(_)
                | FlatPart::Else(_)
                | FlatPart::Endfor(_)
                | FlatPart::Endif(_)
        )
    }

    // (parts_index, bytes_to_strip) for one side of an alone-on-line plan.
    type SideStrip = Option<(usize, usize)>;
    // (back_side, forward_side) plan for a single block tag.
    type StripPlan = (SideStrip, SideStrip);

    // "Alone on line" requires the entire source line containing the tag
    // to consist of only whitespace + this single tag. Scan backwards/forwards
    // through `parts`: a `\n` in a Text segment marks the line boundary; any
    // non-Text (Interp / another tag) encountered before hitting that `\n`
    // disqualifies the tag (it's mid-line). Returns a `StripPlan` to apply,
    // or `None` if not alone.
    fn alone_on_line_strips(parts: &[FlatPart], tag_idx: usize) -> Option<StripPlan> {
        // Scan backwards: look for last `\n` in preceding text, ensuring no
        // non-Text appears between us and that `\n`. The text content from
        // after the `\n` (or literal start) up to the tag must be all ws.
        let back: SideStrip = {
            let mut strip_at: SideStrip = None;
            let mut j = tag_idx;
            while j > 0 {
                j -= 1;
                match &parts[j] {
                    FlatPart::Text(s) => {
                        if let Some(nl_pos) = s.rfind('\n') {
                            let tail = &s[nl_pos + 1..];
                            if !tail.chars().all(|c| c == ' ' || c == '\t') {
                                return None;
                            }
                            let strip = s.len() - (nl_pos + 1);
                            if strip > 0 {
                                strip_at = Some((j, strip));
                            }
                            break;
                        }
                        // No `\n` here — entire text must be ws to keep
                        // scanning further back.
                        if !s.chars().all(|c| c == ' ' || c == '\t') {
                            return None;
                        }
                        if !s.is_empty() {
                            strip_at = Some((j, s.len()));
                        }
                    }
                    // A non-Text part between us and the start-of-line means
                    // there's another tag/interp on the same source line.
                    _ => return None,
                }
            }
            // If we exhausted preceding parts without finding `\n`, the tag
            // is on the first line of the literal — treat literal-start as
            // start-of-line. `strip_at` already captures any leading ws.
            strip_at
        };

        // Scan forwards: look for first `\n` in following text. Everything
        // from the tag to that `\n` (or literal end) must be all ws. The
        // forward strip includes the `\n` itself when found.
        let fwd: SideStrip = {
            let mut strip_at: SideStrip = None;
            let mut j = tag_idx + 1;
            while j < parts.len() {
                match &parts[j] {
                    FlatPart::Text(s) => {
                        if let Some(nl_pos) = s.find('\n') {
                            let prefix = &s[..nl_pos];
                            if !prefix.chars().all(|c| c == ' ' || c == '\t') {
                                return None;
                            }
                            // Include the newline itself.
                            strip_at = Some((j, nl_pos + 1));
                            break;
                        }
                        if !s.chars().all(|c| c == ' ' || c == '\t') {
                            return None;
                        }
                        if !s.is_empty() {
                            strip_at = Some((j, s.len()));
                        }
                    }
                    _ => return None,
                }
                j += 1;
            }
            strip_at
        };

        Some((back, fwd))
    }

    // Compute strips per tag first (immutable scan), then apply in
    // reverse. Adjacent tags can share a Text segment (tag1's forward
    // strip = tag2's back strip on the same index); applying last-to-first
    // means each strip's recorded byte count still references the
    // original content at the edge it's trimming.
    let mut plans: Vec<StripPlan> = Vec::new();
    for i in 0..parts.len() {
        if !is_block_tag(&parts[i]) {
            continue;
        }
        if let Some((back, fwd)) = alone_on_line_strips(parts, i) {
            plans.push((back, fwd));
        }
    }

    for (back, fwd) in plans.into_iter().rev() {
        if let Some((idx, n)) = back {
            if let Some(FlatPart::Text(s)) = parts.get_mut(idx) {
                let new_len = s.len().saturating_sub(n);
                s.truncate(new_len);
            }
        }
        if let Some((idx, n)) = fwd {
            if let Some(FlatPart::Text(s)) = parts.get_mut(idx) {
                let drain_n = n.min(s.len());
                s.drain(..drain_n);
            }
        }
    }

    parts.retain(|p| !matches!(p, FlatPart::Text(s) if s.is_empty()));
}

/// Internal: build a hierarchical `Vec<BacktickSegment>` from a flat
/// open/close stream, recursing into for / if bodies. Stops at a top-level
/// close keyword (`Endfor`, `Endif`, `Else`, `ElseIf`) that the caller will
/// inspect. Returns the segments accumulated for the current frame plus the
/// terminating element (`None` on EOF).
fn build_segment_tree<I: Iterator<Item = FlatPart>>(
    iter: &mut std::iter::Peekable<I>,
) -> Vec<BacktickSegment> {
    let (segs, _term) = build_segment_tree_until(iter, false);
    segs
}

fn build_segment_tree_until<I: Iterator<Item = FlatPart>>(
    iter: &mut std::iter::Peekable<I>,
    stop_on_else: bool,
) -> (Vec<BacktickSegment>, Option<FlatPart>) {
    let mut out: Vec<BacktickSegment> = Vec::new();
    while let Some(part) = iter.next() {
        match part {
            FlatPart::Text(s) => {
                if !s.is_empty() {
                    out.push(BacktickSegment::Text(s));
                }
            }
            FlatPart::Interp(node) => out.push(BacktickSegment::Interp(node)),
            FlatPart::ForOpen(open) => {
                let (body, _) = build_segment_tree_until(iter, false);
                out.push(BacktickSegment::For(BacktickForSegment { open, body }));
            }
            FlatPart::IfOpen(open) => {
                let mut branches: Vec<BacktickIfBranch> = Vec::new();
                let mut else_body: Option<Vec<BacktickSegment>> = None;
                let mut current_header = open;
                loop {
                    let (body, term) = build_segment_tree_until(iter, true);
                    branches.push(BacktickIfBranch {
                        header: current_header,
                        body,
                    });
                    match term {
                        Some(FlatPart::ElseIf(h)) => {
                            current_header = h;
                            continue;
                        }
                        Some(FlatPart::Else(_)) => {
                            let (eb, _) = build_segment_tree_until(iter, false);
                            else_body = Some(eb);
                            break;
                        }
                        _ => break,
                    }
                }
                out.push(BacktickSegment::If(BacktickIfSegment {
                    branches,
                    else_body,
                }));
            }
            // Closing tokens at this level terminate the current frame and
            // bubble up to the caller for matching.
            FlatPart::Endfor(_) | FlatPart::Endif(_) => return (out, Some(part)),
            FlatPart::Else(_) | FlatPart::ElseIf(_) if stop_on_else => return (out, Some(part)),
            // Stray else/else-if outside an if-chain: tree-building treats it as
            // a no-op; `validate_block_structure` reports the diagnostic.
            FlatPart::Else(_) | FlatPart::ElseIf(_) => {}
        }
    }
    (out, None)
}

// Re-export the Flat type from `segments()` so the free functions above can
// reference it. Using a typedef'd public-but-named-private wrapper would
// be cleaner; for now keep the recursion in private free functions and
// hoist the enum.
#[doc(hidden)]
enum FlatPart {
    Text(String),
    Interp(SyntaxNode),
    ForOpen(SyntaxNode),
    Endfor(SyntaxNode),
    IfOpen(SyntaxNode),
    ElseIf(SyntaxNode),
    Else(SyntaxNode),
    Endif(SyntaxNode),
}

/// A structural problem in a backtick template's block tags — an unclosed,
/// mismatched, or stray `${for}`/`${if}` open/close — detected by
/// [`BacktickStringLiteral::segments_with_errors`]. The `span` points at the
/// offending tag (the unmatched open, or the stray/mismatched close).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacktickStructuralError {
    pub kind: BacktickStructuralErrorKind,
    pub span: rowan::TextRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BacktickStructuralErrorKind {
    /// `${for}` with no matching `${endfor}` (span = the `${for}` open).
    UnclosedFor,
    /// `${if}` with no matching `${endif}` (span = the `${if}` open).
    UnclosedIf,
    /// A `${for}` block closed by `${endif}` (span = the `${endif}`).
    MismatchedForClose,
    /// An `${if}` block closed by `${endfor}` (span = the `${endfor}`).
    MismatchedIfClose,
    /// `${endfor}` with no matching `${for}`.
    StrayEndfor,
    /// `${endif}` with no matching `${if}`.
    StrayEndif,
    /// `${else}` outside an `${if}` block.
    StrayElse,
    /// `${else if}` outside an `${if}` block.
    StrayElseIf,
    /// A second `${else}` in the same `${if}` chain.
    DuplicateElse,
    /// `${else if}` appearing after the chain's `${else}`.
    ElseIfAfterElse,
}

/// Stack-based block-tag matcher over the flat part stream — reports unclosed,
/// mismatched, and stray `${for}`/`${if}` tags. Runs alongside (not inside)
/// [`build_segment_tree`], which still builds a best-effort tree for lowering.
fn validate_block_structure(parts: &[FlatPart]) -> Vec<BacktickStructuralError> {
    use BacktickStructuralErrorKind as K;
    #[derive(Clone, Copy, PartialEq)]
    enum Open {
        For,
        /// `seen_else` flips once the chain's `${else}` is matched, so a later
        /// `${else}` / `${else if}` in the same chain can be rejected as
        /// out-of-order rather than silently accepted.
        If {
            seen_else: bool,
        },
    }
    let push = |errors: &mut Vec<BacktickStructuralError>, kind: K, span: rowan::TextRange| {
        errors.push(BacktickStructuralError { kind, span });
    };
    let mut stack: Vec<(Open, rowan::TextRange)> = Vec::new();
    let mut errors: Vec<BacktickStructuralError> = Vec::new();
    for part in parts {
        match part {
            FlatPart::ForOpen(n) => stack.push((Open::For, n.text_range())),
            FlatPart::IfOpen(n) => stack.push((Open::If { seen_else: false }, n.text_range())),
            FlatPart::Endfor(n) => match stack.last() {
                Some((Open::For, _)) => {
                    stack.pop();
                }
                Some((Open::If { .. }, _)) => {
                    stack.pop();
                    push(&mut errors, K::MismatchedIfClose, n.text_range());
                }
                None => push(&mut errors, K::StrayEndfor, n.text_range()),
            },
            FlatPart::Endif(n) => match stack.last() {
                Some((Open::If { .. }, _)) => {
                    stack.pop();
                }
                Some((Open::For, _)) => {
                    stack.pop();
                    push(&mut errors, K::MismatchedForClose, n.text_range());
                }
                None => push(&mut errors, K::StrayEndif, n.text_range()),
            },
            FlatPart::Else(n) => match stack.last_mut() {
                // First `${else}` in the chain: mark it seen.
                Some((Open::If { seen_else }, _)) if !*seen_else => *seen_else = true,
                // A second `${else}` after one already closed the chain's tail.
                Some((Open::If { .. }, _)) => {
                    push(&mut errors, K::DuplicateElse, n.text_range());
                }
                _ => push(&mut errors, K::StrayElse, n.text_range()),
            },
            FlatPart::ElseIf(n) => match stack.last() {
                Some((Open::If { seen_else: false }, _)) => {}
                // `${else if}` after the chain's `${else}` is out of order.
                Some((Open::If { seen_else: true }, _)) => {
                    push(&mut errors, K::ElseIfAfterElse, n.text_range());
                }
                _ => push(&mut errors, K::StrayElseIf, n.text_range()),
            },
            FlatPart::Text(_) | FlatPart::Interp(_) => {}
        }
    }
    // Anything still open at EOF was never closed.
    for (open, span) in stack {
        let kind = match open {
            Open::For => K::UnclosedFor,
            Open::If { .. } => K::UnclosedIf,
        };
        push(&mut errors, kind, span);
    }
    errors
}

/// A piece of a [`BacktickStringLiteral`] after splitting on interpolations
/// and lifting matched block-tag open/close pairs into hierarchical
/// structures.
///
/// Produced by [`BacktickStringLiteral::segments`]. The untagged lowering
/// concatenates `text + interp.to_string()` + control-flow body output;
/// the M4 tagged-template lowering converts these into the BEP §10
/// `parts` / `values` shape.
#[derive(Debug, Clone)]
pub enum BacktickSegment {
    /// Literal text content, escape-decoded and dedent-adjusted.
    Text(String),
    /// A `${expr}` interpolation. The wrapped node is the
    /// `BACKTICK_INTERPOLATION` CST node; downstream code lowers the
    /// inner block expression and converts the result to a string.
    Interp(SyntaxNode),
    /// A `${for (...)}...${endfor}` block. BEP-049 §5. The wrapped node
    /// is the `BACKTICK_FOR_OPEN` CST node (carries the for-header);
    /// `body` is the nested segments between the open and the matching
    /// `${endfor}`.
    For(BacktickForSegment),
    /// A `${if (...)}...${else if (...)}...${else}...${endif}` chain.
    /// Each branch's body is its own segment tree.
    If(BacktickIfSegment),
}

/// Body of a `${for (...)}...${endfor}` block.
#[derive(Debug, Clone)]
pub struct BacktickForSegment {
    /// The `BACKTICK_FOR_OPEN` CST node — caller extracts the for-header.
    pub open: SyntaxNode,
    /// Segments between the open and matching `${endfor}`.
    pub body: Vec<BacktickSegment>,
}

/// Body of a `${if (...)}...${endif}` chain, with optional `else if` and
/// `else` branches.
#[derive(Debug, Clone)]
pub struct BacktickIfSegment {
    /// One entry per `${if}` / `${else if (cond)}` branch, in source order.
    /// `header` is the corresponding `BACKTICK_IF_OPEN` or
    /// `BACKTICK_ELSE_IF` CST node (caller extracts the condition).
    pub branches: Vec<BacktickIfBranch>,
    /// Body of the `${else}` branch, if present.
    pub else_body: Option<Vec<BacktickSegment>>,
}

/// One conditional branch in a [`BacktickIfSegment`].
#[derive(Debug, Clone)]
pub struct BacktickIfBranch {
    pub header: SyntaxNode,
    pub body: Vec<BacktickSegment>,
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
            .find(|token| {
                // `client` is KW_CLIENT in the CST, not WORD.
                token.kind() == SyntaxKind::WORD || token.kind() == SyntaxKind::KW_CLIENT
            })
    }

    /// Get the parameter type.
    pub fn ty(&self) -> Option<TypeExpr> {
        self.syntax.children().find_map(TypeExpr::cast)
    }

    /// Get the default expression syntax element, if present.
    pub fn default_expr_syntax(&self) -> Option<SyntaxElement> {
        let mut seen_equals = false;
        for element in self.syntax.children_with_tokens() {
            match element {
                rowan::NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::EQUALS {
                        seen_equals = true;
                    } else if seen_equals && !token.kind().is_trivia() {
                        return Some(rowan::NodeOrToken::Token(token));
                    }
                }
                rowan::NodeOrToken::Node(node) if seen_equals => {
                    return Some(rowan::NodeOrToken::Node(node));
                }
                rowan::NodeOrToken::Node(_) => {}
            }
        }
        None
    }
}

impl CallArg {
    /// Get the call argument label token from `label = expr`, if present.
    pub fn label(&self) -> Option<SyntaxToken> {
        let tokens: Vec<_> = self
            .syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| !token.kind().is_trivia())
            .collect();

        if tokens.len() >= 2
            && (tokens[0].kind() == SyntaxKind::WORD || tokens[0].kind() == SyntaxKind::KW_CLIENT)
            && tokens[1].kind() == SyntaxKind::EQUALS
        {
            Some(tokens[0].clone())
        } else {
            None
        }
    }

    /// Get the expression node for this call argument, if it was wrapped in a node.
    pub fn expr_syntax(&self) -> Option<SyntaxNode> {
        self.syntax.children().next()
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
    ///
    /// This intentionally excludes methods nested inside `implements` blocks —
    /// those are recovered via [`implements_blocks`](Self::implements_blocks).
    pub fn methods(&self) -> impl Iterator<Item = FunctionDef> {
        self.syntax.children().filter_map(FunctionDef::cast)
    }

    /// Get all `implements I { ... }` blocks declared inside the class body.
    pub fn implements_blocks(&self) -> impl Iterator<Item = ImplementsBlock> {
        self.syntax.children().filter_map(ImplementsBlock::cast)
    }

    /// Get block attributes (@@dynamic).
    pub fn block_attributes(&self) -> impl Iterator<Item = BlockAttribute> {
        self.syntax.children().filter_map(BlockAttribute::cast)
    }
}

impl InterfaceDef {
    /// Get the interface name.
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| {
                token.kind() == SyntaxKind::WORD && token.parent() == Some(self.syntax.clone())
            })
            .nth(0)
    }

    /// Field signatures declared directly in the interface body.
    pub fn fields(&self) -> impl Iterator<Item = Field> {
        self.syntax.children().filter_map(Field::cast)
    }

    /// Associated type declarations declared directly in the interface body.
    pub fn associated_types(&self) -> impl Iterator<Item = AssociatedTypeDecl> {
        self.syntax.children().filter_map(AssociatedTypeDecl::cast)
    }

    /// Default methods declared with a body in the interface.
    pub fn default_methods(&self) -> impl Iterator<Item = FunctionDef> {
        self.syntax.children().filter_map(FunctionDef::cast)
    }

    /// Required method signatures (no body).
    pub fn required_methods(&self) -> impl Iterator<Item = MethodSig> {
        self.syntax.children().filter_map(MethodSig::cast)
    }

    /// Optional `requires I1, I2` clause (BEP-044 canonical form).
    pub fn requires_clause(&self) -> Option<RequiresClause> {
        self.syntax.children().find_map(RequiresClause::cast)
    }
}

impl RequiresClause {
    /// Each `TypeExpr` in the requires clause — one per required interface.
    pub fn parents(&self) -> impl Iterator<Item = TypeExpr> {
        self.syntax.children().filter_map(TypeExpr::cast)
    }
}

impl ImplementsBlock {
    /// The interface this block implements (e.g., `Animal` in `implements Animal { ... }`).
    pub fn target(&self) -> Option<ImplementsTarget> {
        self.syntax.children().find_map(ImplementsTarget::cast)
    }

    /// Field declarations redeclared inside the `implements` block.
    pub fn fields(&self) -> impl Iterator<Item = Field> {
        self.syntax.children().filter_map(Field::cast)
    }

    /// Explicit interface-field links, e.g. `name as display_name`.
    pub fn field_links(&self) -> impl Iterator<Item = InterfaceFieldLink> {
        self.syntax.children().filter_map(InterfaceFieldLink::cast)
    }

    /// Associated type bindings, e.g. `type Item = int`.
    pub fn associated_type_bindings(&self) -> impl Iterator<Item = AssociatedTypeDecl> {
        self.syntax.children().filter_map(AssociatedTypeDecl::cast)
    }

    /// Method definitions (overrides) provided in this block.
    pub fn methods(&self) -> impl Iterator<Item = FunctionDef> {
        self.syntax.children().filter_map(FunctionDef::cast)
    }
}

fn is_member_name_token(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::WORD
            | SyntaxKind::KW_IMPLEMENTS
            | SyntaxKind::KW_IMPLEMENT
            | SyntaxKind::KW_EXTENDS
            | SyntaxKind::KW_REQUIRES
            | SyntaxKind::KW_INTERFACE
    )
}

impl InterfaceFieldLink {
    /// The interface field on the left side of `field as class_field`.
    pub fn interface_field(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| {
                !token.kind().is_trivia()
                    && is_member_name_token(token.kind())
                    && token.kind() != SyntaxKind::KW_AS
            })
    }

    /// The class field on the right side of `field as class_field`.
    pub fn class_field(&self) -> Option<SyntaxToken> {
        let mut after_as = false;
        for token in self
            .syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| !token.kind().is_trivia())
        {
            if token.kind() == SyntaxKind::KW_AS {
                after_as = true;
                continue;
            }
            if after_as && is_member_name_token(token.kind()) {
                return Some(token);
            }
        }
        None
    }

    /// Span of the contextual `as` keyword when present.
    pub fn as_token(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| token.kind() == SyntaxKind::KW_AS)
    }
}

impl ImplementsTarget {
    /// The interface name expression — typically a path optionally with generics.
    pub fn type_expr(&self) -> Option<TypeExpr> {
        self.syntax.children().find_map(TypeExpr::cast)
    }
}

impl ImplementsFor {
    /// The interface being implemented.
    pub fn target(&self) -> Option<ImplementsTarget> {
        self.syntax.children().find_map(ImplementsTarget::cast)
    }

    /// The `for T` target type.
    pub fn for_target(&self) -> Option<ImplementsForTarget> {
        self.syntax.children().find_map(ImplementsForTarget::cast)
    }

    /// Field declarations inside the block.
    pub fn fields(&self) -> impl Iterator<Item = Field> {
        self.syntax.children().filter_map(Field::cast)
    }

    /// Explicit interface-field links, e.g. `name as display_name`.
    pub fn field_links(&self) -> impl Iterator<Item = InterfaceFieldLink> {
        self.syntax.children().filter_map(InterfaceFieldLink::cast)
    }

    /// Associated type bindings, e.g. `type Item = int`.
    pub fn associated_type_bindings(&self) -> impl Iterator<Item = AssociatedTypeDecl> {
        self.syntax.children().filter_map(AssociatedTypeDecl::cast)
    }

    /// Method definitions inside the block.
    pub fn methods(&self) -> impl Iterator<Item = FunctionDef> {
        self.syntax.children().filter_map(FunctionDef::cast)
    }
}

impl ImplementsForTarget {
    pub fn type_expr(&self) -> Option<TypeExpr> {
        self.syntax.children().find_map(TypeExpr::cast)
    }
}

impl AssociatedTypeDecl {
    /// The associated type name after contextual `type`.
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| !token.kind().is_trivia() && token.kind() == SyntaxKind::WORD)
            .find(|token| token.text() != "type")
    }

    /// Optional bound after `extends`.
    pub fn bound(&self) -> Option<TypeExpr> {
        let mut after_extends = false;
        for element in self.syntax.children_with_tokens() {
            match element {
                rowan::NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::KW_EXTENDS {
                        after_extends = true;
                    } else if token.kind() == SyntaxKind::EQUALS {
                        return None;
                    }
                }
                rowan::NodeOrToken::Node(node) if after_extends => {
                    return TypeExpr::cast(node);
                }
                rowan::NodeOrToken::Node(_) => {}
            }
        }
        None
    }

    /// Optional default/binding after `=`.
    pub fn default_or_binding(&self) -> Option<TypeExpr> {
        let mut after_equals = false;
        for element in self.syntax.children_with_tokens() {
            match element {
                rowan::NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::EQUALS {
                        after_equals = true;
                    }
                }
                rowan::NodeOrToken::Node(node) if after_equals => {
                    return TypeExpr::cast(node);
                }
                rowan::NodeOrToken::Node(_) => {}
            }
        }
        None
    }
}

impl MethodSig {
    /// Get the method name.
    ///
    /// Accepts the same keyword tokens as [`FunctionDef::name`] so interface
    /// signatures can use reflection-style names.
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| {
                let kind = token.kind();
                kind == SyntaxKind::WORD
                    || kind == SyntaxKind::KW_IMPLEMENTS
                    || kind == SyntaxKind::KW_IMPLEMENT
                    || kind == SyntaxKind::KW_EXTENDS
                    || kind == SyntaxKind::KW_REQUIRES
                    || kind == SyntaxKind::KW_INTERFACE
            })
    }

    pub fn param_list(&self) -> Option<ParameterList> {
        self.syntax.children().find_map(ParameterList::cast)
    }

    /// Return type — the first `TypeExpr` child that's not inside a parameter.
    pub fn return_type(&self) -> Option<TypeExpr> {
        self.syntax.children().find_map(TypeExpr::cast)
    }

    /// Get the throws clause if present.
    pub fn throws_clause(&self) -> Option<ThrowsClause> {
        self.syntax.children().find_map(ThrowsClause::cast)
    }
}

impl Field {
    /// Get the field name.
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| {
                matches!(
                    token.kind(),
                    SyntaxKind::WORD
                        | SyntaxKind::KW_IMPLEMENTS
                        | SyntaxKind::KW_IMPLEMENT
                        | SyntaxKind::KW_EXTENDS
                        | SyntaxKind::KW_REQUIRES
                        | SyntaxKind::KW_INTERFACE
                        // `client` keyword stays valid as a field name
                        // (BEP-049 §10 `ctx.client` on `Context`).
                        | SyntaxKind::KW_CLIENT
                )
            })
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

impl RetryPolicyDef {
    /// Get the retry policy name.
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| {
                token.kind() == SyntaxKind::WORD && token.parent() == Some(self.syntax.clone())
            })
            .nth(0)
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
    /// Get the config item key (first WORD or keyword token).
    ///
    /// Config items can have keyword tokens as keys (e.g., `retry_policy` inside
    /// a client block is lexed as `KW_RETRY_POLICY`, not `WORD`).
    pub fn key(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| matches!(token.kind(), SyntaxKind::WORD | SyntaxKind::KW_RETRY_POLICY))
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

    /// Get the typed `ConfigValue` child, if present.
    pub fn config_value(&self) -> Option<ConfigValue> {
        self.syntax.children().find_map(ConfigValue::cast)
    }

    /// Get the full config item value as a string.
    /// This handles compound values like "python/pydantic" that span multiple tokens.
    /// Returns the unquoted text of the value.
    pub fn value_str(&self) -> Option<String> {
        self.config_value().and_then(|cv| cv.scalar_text())
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

    /// Get attributes attached to this config item (e.g., `args { ... } @some_attr(...)`).
    pub fn attributes(&self) -> impl Iterator<Item = Attribute> {
        self.syntax.children().filter_map(Attribute::cast)
    }
}

impl ConfigValue {
    /// Extract the unquoted scalar text content, filtering trivia and quotes.
    ///
    /// Returns `None` if the node contains no significant tokens.
    pub fn scalar_text(&self) -> Option<String> {
        let text: String = self
            .syntax
            .descendants_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| !token.kind().is_trivia() && token.kind() != SyntaxKind::QUOTE)
            .map(|token| token.text().to_string())
            .collect();
        if text.is_empty() { None } else { Some(text) }
    }
}

impl TestDef {
    fn function_config_item(&self) -> Option<ConfigItem> {
        self.syntax
            .descendants()
            .filter_map(ConfigItem::cast)
            .find(|item| item.matches_key("functions") || item.matches_key("function"))
    }

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
        self.function_config_item()
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

    /// Get complete function references from the legacy `function(s)` config.
    ///
    /// Unlike [`Self::function_names`], this preserves qualified references
    /// such as `workflows.Classify` as one value.
    pub fn function_reference_names(&self) -> Vec<String> {
        let Some(value) = self
            .function_config_item()
            .and_then(|item| item.config_value_node())
        else {
            return Vec::new();
        };

        let Some(text) = ConfigValue::cast(value).and_then(|value| value.scalar_text()) else {
            return Vec::new();
        };
        let contents = text
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
            .unwrap_or(&text);

        contents
            .split(',')
            .map(str::trim)
            .map(|name| name.trim_matches('"'))
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect()
    }

    /// Get the config block.
    pub fn config_block(&self) -> Option<ConfigBlock> {
        self.syntax.children().find_map(ConfigBlock::cast)
    }
}

impl TypeAliasDef {
    /// Get the type alias name — the first direct WORD child (the `type`
    /// keyword is a `KW_TYPE` token, so no skipping is needed).
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| {
                token.kind() == SyntaxKind::WORD && token.parent() == Some(self.syntax.clone())
            })
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
            .find(|token| {
                matches!(
                    token.kind(),
                    SyntaxKind::WORD | SyntaxKind::KW_DYNAMIC | SyntaxKind::KW_THROWS
                )
            })
    }

    /// Get the full attribute name including dot-separated modifiers.
    /// For @@stream.done returns "stream.done", for @@dynamic returns "dynamic".
    pub fn full_name(&self) -> Option<String> {
        let segments: Vec<String> = self
            .syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| {
                matches!(
                    token.kind(),
                    SyntaxKind::WORD | SyntaxKind::KW_DYNAMIC | SyntaxKind::KW_THROWS
                )
            })
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
                    SyntaxKind::WORD
                        | SyntaxKind::KW_DYNAMIC
                        | SyntaxKind::KW_THROWS
                        | SyntaxKind::DOT
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
        attribute_args_node(&self.syntax).is_some()
    }

    /// Get the text range of the argument node (for error reporting).
    pub fn args_span(&self) -> Option<rowan::TextRange> {
        attribute_args_node(&self.syntax).map(|args| args.text_range())
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
                    return Some(decode_regular_string_literal_text(
                        &child.text().to_string(),
                    ));
                }
                SyntaxKind::RAW_STRING_LITERAL => {
                    if let Some(value) = decode_raw_string_literal_text(&child.text().to_string()) {
                        return Some(value);
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
        attribute_arg_is_string_literal(&self.syntax)
    }

    /// Get all argument nodes in this attribute.
    ///
    /// Each argument is one of:
    /// - `STRING_LITERAL` for `"quoted"`
    /// - `RAW_STRING_LITERAL` for `#"raw"#`
    /// - `EXPR` for `{{ jinja }}`
    /// - `UNQUOTED_STRING` for bare words
    pub fn args(&self) -> impl Iterator<Item = SyntaxNode> + '_ {
        attribute_args(&self.syntax)
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
        attribute_arg_is_string_or_unquoted(&self.syntax)
    }

    /// Check if this attribute has exactly one argument that is a string literal or unquoted string.
    pub fn has_single_string_or_unquoted_arg(&self) -> bool {
        self.arg_count() == 1 && self.arg_is_string_or_unquoted()
    }
}

impl Attribute {
    /// Get the first segment of the attribute name (e.g., "stream" from @stream.done).
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| matches!(token.kind(), SyntaxKind::WORD))
    }

    /// Get the full attribute name including dot-separated modifiers.
    /// For @stream.done returns "stream.done", for @alias returns "alias".
    pub fn full_name(&self) -> Option<String> {
        let segments: Vec<String> = self
            .syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| matches!(token.kind(), SyntaxKind::WORD))
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

    /// Check if attribute has arguments (parentheses with content).
    pub fn has_args(&self) -> bool {
        attribute_args_node(&self.syntax).is_some()
    }

    /// Get the text range of the argument node (for error reporting).
    pub fn args_span(&self) -> Option<rowan::TextRange> {
        attribute_args_node(&self.syntax).map(|args| args.text_range())
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
                    return Some(decode_regular_string_literal_text(
                        &child.text().to_string(),
                    ));
                }
                SyntaxKind::RAW_STRING_LITERAL => {
                    if let Some(value) = decode_raw_string_literal_text(&child.text().to_string()) {
                        return Some(value);
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
        attribute_arg_is_string_literal(&self.syntax)
    }

    /// Get all argument nodes in this attribute.
    ///
    /// Each argument is one of:
    /// - `STRING_LITERAL` for `"quoted"`
    /// - `RAW_STRING_LITERAL` for `#"raw"#`
    /// - `EXPR` for `{{ jinja }}`
    /// - `UNQUOTED_STRING` for bare words
    pub fn args(&self) -> impl Iterator<Item = SyntaxNode> + '_ {
        attribute_args(&self.syntax)
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
        attribute_arg_is_string_or_unquoted(&self.syntax)
    }

    /// Check if this attribute has exactly one argument that is a string literal or unquoted string.
    pub fn has_single_string_or_unquoted_arg(&self) -> bool {
        self.arg_count() == 1 && self.arg_is_string_or_unquoted()
    }

    /// Get the `ATTRIBUTE_ARGS` syntax node as-is (for deferred parsing).
    ///
    /// Returns the raw `SyntaxNode` for the argument list. Used by PPIR to
    /// clone the CST node for deferred parsing in later phases.
    pub fn arg_syntax_node(&self) -> Option<SyntaxNode> {
        attribute_args_node(&self.syntax)
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

impl WhileLetStmt {
    /// Get the refutable pattern (the first `PATTERN` child). Mirrors the
    /// `IF_LET_EXPR` layout: `PATTERN`, scrutinee expr, then `BLOCK_EXPR`.
    pub fn pattern(&self) -> Option<SyntaxNode> {
        self.syntax
            .children()
            .find(|n| n.kind() == SyntaxKind::PATTERN)
    }

    /// Get the scrutinee expression (the expression after `=`). It is the
    /// first child that is neither the `PATTERN` nor the body `BLOCK_EXPR`.
    pub fn scrutinee(&self) -> Option<SyntaxNode> {
        self.syntax
            .children()
            .find(|n| !matches!(n.kind(), SyntaxKind::PATTERN | SyntaxKind::BLOCK_EXPR))
    }

    /// Get the body block expression (the `BLOCK_EXPR` child).
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
                    | SyntaxKind::UPCAST_EXPR
                    | SyntaxKind::OPTIONAL_FIELD_ACCESS_EXPR
                    | SyntaxKind::INDEX_EXPR
                    | SyntaxKind::OPTIONAL_INDEX_EXPR
                    | SyntaxKind::OPTIONAL_CALL_EXPR
                    | SyntaxKind::TAGGED_TEMPLATE_EXPR
                    | SyntaxKind::IF_EXPR
                    | SyntaxKind::IF_LET_EXPR
                    | SyntaxKind::MATCH_EXPR
                    | SyntaxKind::CATCH_EXPR
                    | SyntaxKind::THROW_EXPR
                    | SyntaxKind::SPAWN_EXPR
                    | SyntaxKind::AWAIT_EXPR
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

impl ThrowStmt {
    /// Get the throw expression node.
    pub fn expr(&self) -> Option<ThrowExpr> {
        self.syntax.children().find_map(ThrowExpr::cast)
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
                if matches!(
                    node.kind(),
                    SyntaxKind::WHILE_STMT
                        | SyntaxKind::WHILE_LET_STMT
                        | SyntaxKind::FOR_EXPR
                        | SyntaxKind::DEFER_STMT
                ) {
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
                        | SyntaxKind::RETURN_STMT
                        | SyntaxKind::WHILE_STMT
                        | SyntaxKind::WHILE_LET_STMT
                        | SyntaxKind::FOR_EXPR
                        | SyntaxKind::BREAK_STMT
                        | SyntaxKind::CONTINUE_STMT
                        | SyntaxKind::THROW_STMT
                        | SyntaxKind::DEFER_STMT
                        // test/testset declarations inside blocks (dynamic test generation)
                        | SyntaxKind::TEST_EXPR_DEF
                        | SyntaxKind::TESTSET_DEF => Some(BlockElement::Stmt(n)),
                        // Header comment (//# name)
                        SyntaxKind::HEADER_COMMENT => Some(BlockElement::HeaderComment(n)),
                        // Expression nodes
                        SyntaxKind::EXPR
                        | SyntaxKind::BINARY_EXPR
                        | SyntaxKind::IS_EXPR
                        | SyntaxKind::UNARY_EXPR
                        | SyntaxKind::CALL_EXPR
                        | SyntaxKind::IF_EXPR
                        | SyntaxKind::IF_LET_EXPR
                        | SyntaxKind::MATCH_EXPR
                        | SyntaxKind::CATCH_EXPR
                        | SyntaxKind::THROW_EXPR
                        | SyntaxKind::SPAWN_EXPR
                        | SyntaxKind::AWAIT_EXPR
                        | SyntaxKind::BLOCK_EXPR
                        | SyntaxKind::PATH_EXPR
                        | SyntaxKind::FIELD_ACCESS_EXPR
                        | SyntaxKind::UPCAST_EXPR
                        | SyntaxKind::OPTIONAL_FIELD_ACCESS_EXPR
                        | SyntaxKind::ENV_ACCESS_EXPR
                        | SyntaxKind::INDEX_EXPR
                        | SyntaxKind::OPTIONAL_INDEX_EXPR
                        | SyntaxKind::OPTIONAL_CALL_EXPR
                        | SyntaxKind::PAREN_EXPR
                        | SyntaxKind::ARRAY_LITERAL
                        | SyntaxKind::OBJECT_LITERAL
                        | SyntaxKind::MAP_LITERAL
                        | SyntaxKind::STRING_LITERAL
                        // A lambda can be a block's tail expression — e.g. a
                        // function whose body returns a middleware transformer
                        // (BEP-034). Without this it was silently dropped and
                        // the block typed as void ("missing return value").
                        | SyntaxKind::LAMBDA_EXPR
                        | SyntaxKind::RAW_STRING_LITERAL
                        | SyntaxKind::BACKTICK_STRING_LITERAL
                        | SyntaxKind::TAGGED_TEMPLATE_EXPR => Some(BlockElement::ExprNode(n)),
                        _ => None,
                    }
                }
                rowan::NodeOrToken::Token(t) => {
                    // Keep literals and identifiers (potential tail expressions)
                    match t.kind() {
                        SyntaxKind::WORD
                        | SyntaxKind::BIGINT_LITERAL
                        | SyntaxKind::INTEGER_LITERAL
                        | SyntaxKind::FLOAT_LITERAL
                        | SyntaxKind::STRING_LITERAL
                        | SyntaxKind::RAW_STRING_LITERAL
                        // Boolean / null literals are re-lexed contextual keywords.
                        | SyntaxKind::KW_TRUE
                        | SyntaxKind::KW_FALSE
                        | SyntaxKind::KW_NULL => Some(BlockElement::ExprToken(t)),
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
    ///
    /// The interface-related keywords (`implements`, `interface`, `extends`)
    /// also count: they remain callable as member names on the reflection
    /// `type` value (e.g. `dog_t.implements(animal_t)`).
    pub fn field(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| {
                matches!(
                    token.kind(),
                    SyntaxKind::WORD
                        | SyntaxKind::KW_IMPLEMENTS
                        | SyntaxKind::KW_IMPLEMENT
                        | SyntaxKind::KW_INTERFACE
                        | SyntaxKind::KW_EXTENDS
                        | SyntaxKind::KW_REQUIRES
                )
            })
            .last() // The field name is the last member-name token
    }
}

impl EnvAccessExpr {
    /// Get the field name (the env var name or method name after `env.`).
    /// Skips the leading `"env"` WORD and finds the WORD after the DOT.
    pub fn field(&self) -> Option<SyntaxToken> {
        let mut seen_dot = false;
        for elem in self.syntax.children_with_tokens() {
            if let rowan::NodeOrToken::Token(t) = elem {
                if t.kind() == SyntaxKind::DOT {
                    seen_dot = true;
                } else if seen_dot && t.kind() == SyntaxKind::WORD {
                    return Some(t);
                }
            }
        }
        None
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

impl ThrowExpr {
    /// Get the thrown expression/value.
    pub fn value(&self) -> Option<SyntaxNode> {
        self.syntax.children().next()
    }
}

impl ReturnExpr {
    /// Get the returned value expression, if present (bare `return` has none).
    pub fn value(&self) -> Option<SyntaxNode> {
        self.syntax.children().next()
    }
}

impl ThrowsClause {
    /// Get the type expression for the throws clause.
    pub fn type_expr(&self) -> Option<TypeExpr> {
        self.syntax.children().find_map(TypeExpr::cast)
    }
}

impl CatchExpr {
    /// Get the base expression before the first catch clause.
    pub fn base(&self) -> Option<SyntaxNode> {
        self.syntax
            .children()
            .find(|n| n.kind() != SyntaxKind::CATCH_CLAUSE)
    }

    /// Iterate over attached catch clauses in source order.
    pub fn clauses(&self) -> impl Iterator<Item = CatchClause> + '_ {
        self.syntax.children().filter_map(CatchClause::cast)
    }
}

impl CatchClause {
    /// Get the clause keyword token (`catch`, `catch_all`, `catch_all_panics`).
    pub fn keyword(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| {
                matches!(
                    t.kind(),
                    SyntaxKind::KW_CATCH
                        | SyntaxKind::KW_CATCH_ALL
                        | SyntaxKind::KW_CATCH_ALL_PANICS
                )
            })
    }

    /// Get the binding pattern from `catch (...)`.
    pub fn binding(&self) -> Option<CatchPattern> {
        self.syntax.children().find_map(CatchPattern::cast)
    }

    /// Get the optional stack trace binding node from `catch (e, st)`.
    pub fn stack_trace_binding(&self) -> Option<SyntaxNode> {
        self.syntax
            .children()
            .find(|c| c.kind() == SyntaxKind::CATCH_STACK_TRACE_BINDING)
    }

    /// Iterate over typed/fallback arm entries for this clause.
    pub fn arms(&self) -> impl Iterator<Item = CatchArm> + '_ {
        self.syntax.children().filter_map(CatchArm::cast)
    }
}

impl CatchArm {
    /// Get the pattern for this catch arm.
    pub fn pattern(&self) -> Option<CatchPattern> {
        self.syntax.children().find_map(CatchPattern::cast)
    }

    /// Get the body expression of this arm.
    pub fn body(&self) -> Option<SyntaxNode> {
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

    /// Check if this catch arm has a block body.
    pub fn has_block_body(&self) -> bool {
        self.body()
            .map(|n| n.kind() == SyntaxKind::BLOCK_EXPR)
            .unwrap_or(false)
    }
}

impl CatchPattern {
    /// Check if this is a union pattern (has `|` separators).
    pub fn is_union(&self) -> bool {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .any(|token| token.kind() == SyntaxKind::PIPE)
    }

    /// Check if this is a typed binding pattern (has `:`).
    pub fn is_typed_binding(&self) -> bool {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .any(|token| token.kind() == SyntaxKind::COLON)
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

    /// Get the binding name for this pattern.
    pub fn binding_name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| token.kind() == SyntaxKind::WORD)
    }

    /// Get the type expression for typed bindings.
    pub fn binding_type(&self) -> Option<TypeExpr> {
        self.syntax.children().find_map(TypeExpr::cast)
    }
}

/// Enum for any top-level item.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Item {
    Function(FunctionDef),
    Class(ClassDef),
    Enum(EnumDef),
    Interface(InterfaceDef),
    ImplementsFor(ImplementsFor),
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
                | SyntaxKind::INTERFACE_DEF
                | SyntaxKind::IMPLEMENTS_FOR
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
            SyntaxKind::INTERFACE_DEF => Some(Item::Interface(InterfaceDef { syntax })),
            SyntaxKind::IMPLEMENTS_FOR => Some(Item::ImplementsFor(ImplementsFor { syntax })),
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
            Item::Interface(it) => it.syntax(),
            Item::ImplementsFor(it) => it.syntax(),
            Item::Client(it) => it.syntax(),
            Item::Test(it) => it.syntax(),
            Item::RetryPolicy(it) => it.syntax(),
            Item::TemplateString(it) => it.syntax(),
            Item::TypeAlias(it) => it.syntax(),
        }
    }
}
