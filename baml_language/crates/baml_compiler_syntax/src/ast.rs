//! Typed AST node wrappers for ergonomic tree access.

use rowan::ast::AstNode;

use crate::{SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};

/// Extract a dotted name from a token sequence (e.g., `baml.http.Request` → `"baml.http.Request"`).
///
/// Finds the first WORD token, then consumes alternating DOT + identifier
/// segments. Declaration keywords are identifiers only after a dot.
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

    // Consume alternating DOT + identifier segments, mirroring the parser's
    // qualified-name carve-out.
    while let Some(t) = iter.next() {
        if t.kind() != SyntaxKind::DOT {
            break;
        }
        let Some(word) = iter.next() else { break };
        if !matches!(
            word.kind(),
            SyntaxKind::WORD
                | SyntaxKind::KW_SPAWN
                | SyntaxKind::KW_AWAIT
                | SyntaxKind::KW_CLASS
                | SyntaxKind::KW_ENUM
                | SyntaxKind::KW_INTERFACE
                | SyntaxKind::KW_FUNCTION
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
ast_node!(ToolsField, TOOLS_FIELD);
ast_node!(SpecExpr, SPEC_EXPR);
ast_node!(ClientValueDef, CLIENT_VALUE_DEF);
ast_node!(RawStringLiteral, RAW_STRING_LITERAL);
ast_node!(StringLiteral, STRING_LITERAL);
ast_node!(BacktickStringLiteral, BACKTICK_STRING_LITERAL);
ast_node!(BacktickText, BACKTICK_TEXT);
ast_node!(BacktickInterpolation, BACKTICK_INTERPOLATION);

ast_node!(TypeExpr, TYPE_EXPR);
ast_node!(Attribute, ATTRIBUTE);
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
            .take_while(|t| {
                matches!(
                    t.kind(),
                    SyntaxKind::WORD
                        | SyntaxKind::DOT
                        | SyntaxKind::KW_SPAWN
                        | SyntaxKind::KW_AWAIT
                        | SyntaxKind::KW_CLASS
                        | SyntaxKind::KW_ENUM
                        | SyntaxKind::KW_INTERFACE
                        | SyntaxKind::KW_FUNCTION
                )
            })
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

    /// Get the string literal value if this member is a string literal type.
    pub fn string_literal(&self) -> Option<String> {
        self.child_nodes
            .iter()
            .find(|n| n.kind() == SyntaxKind::STRING_LITERAL)
            .map(|n| decode_regular_string_literal_text(&n.text().to_string()))
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
ast_node!(BlockExpr, BLOCK_EXPR);
ast_node!(ReturnStmt, RETURN_STMT);
ast_node!(ThrowStmt, THROW_STMT);
ast_node!(BreakStmt, BREAK_STMT);
ast_node!(ContinueStmt, CONTINUE_STMT);
ast_node!(DeferStmt, DEFER_STMT);
ast_node!(PathExpr, PATH_EXPR);
ast_node!(FieldAccessExpr, FIELD_ACCESS_EXPR);
ast_node!(UpcastExpr, UPCAST_EXPR);
ast_node!(QualifiedPathExpr, QUALIFIED_PATH_EXPR);
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
    /// without renaming. `match` is admitted for the same reason, for
    /// `baml.regex.Regex.match`.
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
                    || kind == SyntaxKind::KW_INTERFACE
                    || kind == SyntaxKind::KW_MATCH;
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

    /// Get the LLM function body if this is an LLM function.
    pub fn llm_body(&self) -> Option<LlmFunctionBody> {
        self.syntax.children().find_map(LlmFunctionBody::cast)
    }

    /// Get the expression function body if this is an expression function.
    pub fn expr_body(&self) -> Option<ExprFunctionBody> {
        self.syntax.children().find_map(ExprFunctionBody::cast)
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
    /// For `function Foo() -> string { client: GPT4 ... }`, returns the `client: GPT4` field.
    pub fn client_field(&self) -> Option<ClientField> {
        self.syntax.children().find_map(ClientField::cast)
    }

    /// Get the prompt field if present.
    ///
    /// For `function Foo() -> string { ... prompt: #"..."# }`, returns the `prompt: #"..."#` field.
    pub fn prompt_field(&self) -> Option<PromptField> {
        self.syntax.children().find_map(PromptField::cast)
    }

    /// Get the tools field if present.
    ///
    /// For `function Foo() -> T { ... tools: [a, b] ... }`, returns the
    /// `tools: [a, b]` field.
    pub fn tools_field(&self) -> Option<ToolsField> {
        self.syntax.children().find_map(ToolsField::cast)
    }
}

impl ClientValueDef {
    /// The declared client name (`Fast` in `client Fast = <expr>;`).
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| t.kind() == SyntaxKind::WORD)
    }

    /// The initializer — the first element after the `=` (a node, or a bare
    /// identifier/literal token).
    pub fn value_element(&self) -> Option<rowan::NodeOrToken<SyntaxNode, SyntaxToken>> {
        let mut seen_equals = false;
        for el in self.syntax.children_with_tokens() {
            match &el {
                rowan::NodeOrToken::Node(_) if seen_equals => return Some(el),
                rowan::NodeOrToken::Token(t) => {
                    if t.kind() == SyntaxKind::EQUALS {
                        seen_equals = true;
                        continue;
                    }
                    if seen_equals && !t.kind().is_trivia() && t.kind() != SyntaxKind::SEMICOLON {
                        return Some(el);
                    }
                }
                rowan::NodeOrToken::Node(_) => {}
            }
        }
        None
    }
}

impl ToolsField {
    /// The tools value expression — the first child node after the `tools`
    /// keyword and colon (usually an `ARRAY_LITERAL`).
    pub fn expr(&self) -> Option<SyntaxNode> {
        self.syntax.children().next()
    }

    /// The tools value as a node-or-token element. A bare dot-free
    /// identifier (`tools: my_tools`) is emitted by the parser as a WORD
    /// token with no wrapping node, so [`Self::expr`] alone would miss it
    /// and the field would silently lower to an empty toolbox.
    pub fn value_element(&self) -> Option<rowan::NodeOrToken<SyntaxNode, SyntaxToken>> {
        let mut seen_keyword = false;
        for el in self.syntax.children_with_tokens() {
            match &el {
                rowan::NodeOrToken::Node(_) => return Some(el),
                rowan::NodeOrToken::Token(t) => {
                    if t.kind().is_trivia() || t.kind() == SyntaxKind::COLON {
                        continue;
                    }
                    // The leading `tools` keyword lexes as a WORD; everything
                    // after it (and the colon) is the value.
                    if !seen_keyword && t.kind() == SyntaxKind::WORD && t.text() == "tools" {
                        seen_keyword = true;
                        continue;
                    }
                    return Some(el);
                }
            }
        }
        None
    }
}

impl SpecExpr {
    /// The base expression the postfix `@spec` was applied to (a `PATH_EXPR`
    /// naming an LLM function).
    pub fn base(&self) -> Option<SyntaxNode> {
        self.syntax.children().next()
    }
}

impl ClientField {
    /// Get the client name token if it's a simple identifier.
    ///
    /// For `client: GPT4`, returns the `GPT4` token.
    /// For `client: "openai/gpt-4o"`, returns None (use `name_or_string()` instead).
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| token.kind() == SyntaxKind::WORD)
    }

    /// Get the client value as a string, whether it's an identifier, an
    /// unquoted shorthand, or a string literal.
    ///
    /// For `client: GPT4`, returns "GPT4".
    /// For `client: "openai/gpt-4o"`, returns "openai/gpt-4o".
    /// For `client: openai/gpt-4o` (unquoted shorthand), returns
    /// "openai/gpt-4o" — the parser consumes the whole shorthand as value
    /// tokens, and truncating to the first WORD would silently resolve the
    /// provider prefix alone.
    pub fn value(&self) -> Option<String> {
        // Try token form first: concatenate every non-trivia value token after
        // the `client` keyword and the leading colon. Only the FIRST
        // colon is field syntax — later ones belong to the value (model ids
        // like `ollama/llama3:8b`). A single WORD yields the plain identifier;
        // a multi-token run reproduces the unquoted shorthand (its source has
        // no interior whitespace).
        let mut value = String::new();
        let mut leading_colon_eaten = false;
        for token in self
            .syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|t| !t.kind().is_trivia() && t.kind() != SyntaxKind::KW_CLIENT)
        {
            if token.kind() == SyntaxKind::COLON && value.is_empty() && !leading_colon_eaten {
                leading_colon_eaten = true;
                continue;
            }
            value.push_str(token.text());
        }
        if !value.is_empty() {
            return Some(value);
        }

        // Otherwise, try to get it as a string literal
        if let Some(string_node) = self.syntax.children().find_map(StringLiteral::cast) {
            return Some(string_node.value());
        }

        None
    }

    /// The client value as a node-or-token element: a `STRING_LITERAL` node
    /// for the `"provider/model"` form, any other node for an expression
    /// (`client: my_client()`), or a bare identifier token (`client: Fast`).
    pub fn value_element(&self) -> Option<rowan::NodeOrToken<SyntaxNode, SyntaxToken>> {
        for el in self.syntax.children_with_tokens() {
            match &el {
                rowan::NodeOrToken::Node(_) => return Some(el),
                rowan::NodeOrToken::Token(t) => {
                    if t.kind().is_trivia()
                        || matches!(t.kind(), SyntaxKind::KW_CLIENT | SyntaxKind::COLON)
                    {
                        continue;
                    }
                    return Some(el);
                }
            }
        }
        None
    }
}

impl PromptField {
    /// Get a legacy raw-string prompt for migration diagnostics.
    pub fn raw_string(&self) -> Option<RawStringLiteral> {
        self.syntax.children().find_map(RawStringLiteral::cast)
    }

    /// Get the backtick string literal node containing the prompt.
    ///
    /// For `` prompt: `Hello ${name}` ``, returns the `` `Hello ${name}` `` node.
    /// A backtick prompt compiles to a prompt-tag closure.
    pub fn backtick_string(&self) -> Option<BacktickStringLiteral> {
        self.syntax.children().find_map(BacktickStringLiteral::cast)
    }

    /// Get the quoted string literal node containing the prompt.
    ///
    /// For `prompt "Hello"`, returns the `"Hello"` node. A quoted prompt
    /// compiles to the same prompt-tag closure as a backtick one, with the
    /// whole literal as a single text segment — `"..."` does not interpolate.
    pub fn string(&self) -> Option<StringLiteral> {
        self.syntax.children().find_map(StringLiteral::cast)
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

    /// Split the literal into the alternating text and interpolation segments
    /// that downstream lowerers consume.
    ///
    /// For `` `Hello, ${user.name}!` `` returns:
    /// `[Text("Hello, "), Interp(<${user.name}>), Text("!")]`.
    ///
    /// Multi-line content is dedented per BEP §12 (see
    /// [`baml_base::dedent::dedent_backtick`]) with interpolations excluded from
    /// the min-indent calculation (§12 rule 8 — "Whitespace inside `${...}` is
    /// preserved verbatim"), §13 block-tag whitespace control is applied, and
    /// then text segments are escape-decoded. Only *layout* is stripped: an
    /// authored `\n` at the end of a literal survives.
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
    /// stream (text + interp + block-tag opens/closes) with whole-literal §12
    /// dedent, §13 whitespace control, and escape decoding — in that order.
    /// Pass (2) — lifting matched open/close pairs into nested For / If
    /// segments — is `build_segment_tree`.
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

        // Everything from here through §13 whitespace control runs on the *raw*
        // chunk text, with escapes still encoded. Both are source-layout rules,
        // and an authored `\n` is content, not layout. Decoding first would turn
        // it into a real newline that the dedent's edge handling and §13's
        // line scan could then read as a line break of the layout — which is
        // exactly how `` `${host}\n` `` used to lose its trailing newline.

        // §12 dedent across the whole literal. Replace each non-text part with
        // a single-char placeholder so an interpolation neither contributes to
        // the min-indent nor has its own lines re-indented (§12 rule 8:
        // "Whitespace inside `${...}` is preserved verbatim"), then split the
        // dedented result back into text segments and reattach the parts in
        // order.
        let needs_dedent = parts
            .iter()
            .any(|p| matches!(p, FlatPart::Text(s) if s.contains(['\n', '\r'])));
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
            let dedented = baml_base::dedent::dedent_backtick(&joined);
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

        // Escapes decode last, once every layout rule has run. `\n`/`\t`
        // sequences produced here are user content and are never trimmed.
        for p in &mut parts {
            if let FlatPart::Text(s) = p {
                *s = unescape_backtick_string_literal(s);
            }
        }
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

    /// Get all variants.
    pub fn variants(&self) -> impl Iterator<Item = EnumVariant> {
        self.syntax.children().filter_map(EnumVariant::cast)
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
    /// Get the first segment of the attribute name (e.g. `stream` from `@@stream.done`).
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| matches!(token.kind(), SyntaxKind::WORD | SyntaxKind::KW_THROWS))
    }

    /// Get the full attribute name including dot-separated modifiers.
    /// For `@@stream.done`, returns `stream.done`.
    pub fn full_name(&self) -> Option<String> {
        let segments: Vec<String> = self
            .syntax
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| matches!(token.kind(), SyntaxKind::WORD | SyntaxKind::KW_THROWS))
            .map(|token| token.text().to_string())
            .collect();

        if segments.is_empty() {
            None
        } else {
            Some(segments.join("."))
        }
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
                        | SyntaxKind::TYPE_BINDING_STMT
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
                        | SyntaxKind::QUALIFIED_PATH_EXPR
                        | SyntaxKind::SPEC_EXPR
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

impl ThrowsClause {
    /// Get the type expression for the throws clause.
    pub fn type_expr(&self) -> Option<TypeExpr> {
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
            Item::RetryPolicy(it) => it.syntax(),
            Item::TemplateString(it) => it.syntax(),
            Item::TypeAlias(it) => it.syntax(),
        }
    }
}
