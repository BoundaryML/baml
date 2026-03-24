//! Semantic tokens for BAML files.
//!
//! NOTE: HIR/TIR-based resolution is stubbed — pending compiler2 LSP action reimplementation.
//! The CST-based token classification is fully functional.

use baml_db::{
    SourceFile, baml_compiler_parser,
    baml_compiler_syntax::{SyntaxKind, SyntaxNode, SyntaxToken},
};
use baml_project::ProjectDatabase;
use rowan::NodeOrToken;
use text_size::TextRange;

/// The semantic token type for a BAML file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticTokenType {
    Namespace,
    Type,
    Class,
    Enum,
    Interface,
    Struct,
    TypeParameter,
    Parameter,
    Variable,
    Property,
    EnumMember,
    Event,
    Function,
    Method,
    Macro,
    Keyword,
    Modifier,
    Comment,
    String,
    Number,
    Regexp,
    Operator,
    Decorator,
}

/// Token type legend order
pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::Namespace,
    SemanticTokenType::Type,
    SemanticTokenType::Class,
    SemanticTokenType::Enum,
    SemanticTokenType::Interface,
    SemanticTokenType::Struct,
    SemanticTokenType::TypeParameter,
    SemanticTokenType::Parameter,
    SemanticTokenType::Variable,
    SemanticTokenType::Property,
    SemanticTokenType::EnumMember,
    SemanticTokenType::Event,
    SemanticTokenType::Function,
    SemanticTokenType::Method,
    SemanticTokenType::Macro,
    SemanticTokenType::Keyword,
    SemanticTokenType::Modifier,
    SemanticTokenType::Comment,
    SemanticTokenType::String,
    SemanticTokenType::Number,
    SemanticTokenType::Regexp,
    SemanticTokenType::Operator,
    SemanticTokenType::Decorator,
];

impl SemanticTokenType {
    /// Get the index of the token type in the legend.
    #[allow(clippy::cast_possible_truncation)]
    pub fn legend_index(self) -> u32 {
        TOKEN_TYPES
            .iter()
            .position(|t| *t == self)
            .expect("SemanticTokenType missing in legend") as u32
    }

    /// Get the string representation of the token type.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Namespace => "namespace",
            Self::Type => "type",
            Self::Class => "class",
            Self::Enum => "enum",
            Self::Interface => "interface",
            Self::Struct => "struct",
            Self::TypeParameter => "typeParameter",
            Self::Parameter => "parameter",
            Self::Variable => "variable",
            Self::Property => "property",
            Self::EnumMember => "enumMember",
            Self::Event => "event",
            Self::Function => "function",
            Self::Method => "method",
            Self::Macro => "macro",
            Self::Keyword => "keyword",
            Self::Modifier => "modifier",
            Self::Comment => "comment",
            Self::String => "string",
            Self::Number => "number",
            Self::Regexp => "regexp",
            Self::Operator => "operator",
            Self::Decorator => "decorator",
        }
    }
}

/// A semantic token for a BAML file.
#[derive(Debug, Clone)]
pub struct SemanticToken {
    pub range: TextRange,
    pub token_type: SemanticTokenType,
}

/// Emit a semantic token for a single leaf token.
fn emit_token(token: &SyntaxToken, token_type: SemanticTokenType, out: &mut Vec<SemanticToken>) {
    if !token.kind().is_whitespace() {
        out.push(SemanticToken {
            range: token.text_range(),
            token_type,
        });
    }
}

/// Emit a token type for all non-trivia leaf tokens under a node.
fn emit_node(node: &SyntaxNode, token_type: SemanticTokenType, out: &mut Vec<SemanticToken>) {
    for child in node.descendants_with_tokens() {
        if let NodeOrToken::Token(t) = child {
            emit_token(&t, token_type, out);
        }
    }
}

/// Emit semantic tokens for a single file. Always returns semantic tokens in document order.
pub fn semantic_tokens(db: &ProjectDatabase, file: SourceFile) -> Vec<SemanticToken> {
    let root = baml_compiler_parser::syntax_tree(db, file);
    let mut out = Vec::new();
    visit_node(db, file, &root, &mut out);
    out
}

/// Dispatch a single node to its visitor.
fn visit_node(
    db: &ProjectDatabase,
    file: SourceFile,
    node: &SyntaxNode,
    out: &mut Vec<SemanticToken>,
) {
    match node.kind() {
        ref n if n.is_comment() => emit_node(node, SemanticTokenType::Comment, out),
        // String literals are nodes, emit the whole thing with all its children as string.
        SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
            emit_node(node, SemanticTokenType::String, out);
        }
        SyntaxKind::ATTRIBUTE | SyntaxKind::BLOCK_ATTRIBUTE => visit_attribute(db, file, node, out),
        SyntaxKind::TYPE_ALIAS_DEF => visit_type_alias_def(db, file, node, out),
        SyntaxKind::ENUM_DEF => visit_word_as(db, file, node, SemanticTokenType::Enum, out),
        SyntaxKind::ENUM_VARIANT => {
            visit_word_as(db, file, node, SemanticTokenType::EnumMember, out);
        }
        SyntaxKind::CLASS_DEF => visit_word_as(db, file, node, SemanticTokenType::Class, out),
        SyntaxKind::FIELD => visit_word_as(db, file, node, SemanticTokenType::Property, out),
        SyntaxKind::FUNCTION_DEF => visit_function_def(db, file, node, out),
        SyntaxKind::PARAMETER => visit_word_as(db, file, node, SemanticTokenType::Parameter, out),
        SyntaxKind::TYPE_EXPR => visit_type_expr(db, file, node, out),
        // Highlight top-level let statements for now...
        SyntaxKind::LET_STMT => {
            visit_first_word_as(db, file, node, SemanticTokenType::Variable, out);
        }
        SyntaxKind::CLIENT_TYPE => visit_word_as(db, file, node, SemanticTokenType::Type, out),
        SyntaxKind::CONFIG_ITEM => visit_config_item(db, file, node, out),
        // Put these as struct so they're in theory different from classes
        SyntaxKind::CLIENT_DEF | SyntaxKind::GENERATOR_DEF | SyntaxKind::RETRY_POLICY_DEF => {
            visit_word_as(db, file, node, SemanticTokenType::Struct, out);
        }
        SyntaxKind::TEST_DEF => visit_word_as(db, file, node, SemanticTokenType::Struct, out),
        // I guess this is sorta like a function?
        SyntaxKind::TEMPLATE_STRING_DEF => {
            visit_word_as(db, file, node, SemanticTokenType::Function, out);
        }
        SyntaxKind::PROMPT_FIELD => visit_word_as(db, file, node, SemanticTokenType::Property, out),
        SyntaxKind::CLIENT_FIELD => visit_client_field(db, file, node, out),
        _ => visit_children(db, file, node, out),
    }
}

/// Classify a leaf token into a semantic token type.
fn visit_token(token: &SyntaxToken, out: &mut Vec<SemanticToken>) {
    out.push(SemanticToken {
        range: token.text_range(),
        token_type: match token.kind() {
            ref kind if kind.is_whitespace() => return,
            ref kind if kind.is_keyword() => SemanticTokenType::Keyword,
            ref kind if kind.is_operator() => SemanticTokenType::Operator,
            ref kind if kind.is_comment() => SemanticTokenType::Comment,
            SyntaxKind::INTEGER_LITERAL | SyntaxKind::FLOAT_LITERAL => SemanticTokenType::Number,
            _ => return,
        },
    });
}

/// Walk all children of a node, dispatching child nodes via `visit_node`
/// and classifying leaf tokens via `visit_token`.
fn visit_children(
    db: &ProjectDatabase,
    file: SourceFile,
    node: &SyntaxNode,
    out: &mut Vec<SemanticToken>,
) {
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, file, &n, out),
            NodeOrToken::Token(t) => visit_token(&t, out),
        }
    }
}

/// Visit a node where all WORD tokens should be classified as `word_type`.
fn visit_word_as(
    db: &ProjectDatabase,
    file: SourceFile,
    node: &SyntaxNode,
    word_type: SemanticTokenType,
    out: &mut Vec<SemanticToken>,
) {
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, file, &n, out),
            NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::WORD => emit_token(&t, word_type, out),
                _ => visit_token(&t, out),
            },
        }
    }
}

/// Visit a node where the first WORD token should be classified as `word_type`.
fn visit_first_word_as(
    db: &ProjectDatabase,
    file: SourceFile,
    node: &SyntaxNode,
    word_type: SemanticTokenType,
    out: &mut Vec<SemanticToken>,
) {
    let mut found_word = false;
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, file, &n, out),
            NodeOrToken::Token(t) => {
                if !found_word && t.kind() == SyntaxKind::WORD {
                    found_word = true;
                    emit_token(&t, word_type, out);
                } else {
                    visit_token(&t, out);
                }
            }
        }
    }
}

/// Visit a `CONFIG_ITEM` node, classifying the key as a property.
fn visit_config_item(
    db: &ProjectDatabase,
    file: SourceFile,
    node: &SyntaxNode,
    out: &mut Vec<SemanticToken>,
) {
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, file, &n, out),
            NodeOrToken::Token(t) => match t.kind() {
                // Handle keywords like `retry_policy` as properties.
                ref k if k.is_keyword() => emit_token(&t, SemanticTokenType::Property, out),
                SyntaxKind::WORD => emit_token(&t, SemanticTokenType::Property, out),
                _ => visit_token(&t, out),
            },
        }
    }
}

/// Visit a `CLIENT_FIELD` node, classifying the client name as a property.
fn visit_client_field(
    db: &ProjectDatabase,
    file: SourceFile,
    node: &SyntaxNode,
    out: &mut Vec<SemanticToken>,
) {
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, file, &n, out),
            NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::KW_CLIENT => emit_token(&t, SemanticTokenType::Property, out),
                _ => visit_token(&t, out),
            },
        }
    }
}

/// Visit an attribute or a block attribute node.
fn visit_attribute(
    db: &ProjectDatabase,
    file: SourceFile,
    node: &SyntaxNode,
    out: &mut Vec<SemanticToken>,
) {
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, file, &n, out),
            NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::AT_AT | SyntaxKind::AT | SyntaxKind::WORD => {
                    emit_token(&t, SemanticTokenType::Decorator, out);
                }
                ref k if k.is_keyword() => emit_token(&t, SemanticTokenType::Decorator, out),
                _ => visit_token(&t, out),
            },
        }
    }
}

/// Visit a `TYPE_ALIAS_DEF` node, classifying "type" as a keyword, and the type name as a type.
fn visit_type_alias_def(
    db: &ProjectDatabase,
    file: SourceFile,
    node: &SyntaxNode,
    out: &mut Vec<SemanticToken>,
) {
    let mut found_keyword = false; // "type" is not actually a keyword, it's a WORD token.
    let mut found_name = false;
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, file, &n, out),
            NodeOrToken::Token(t) => {
                if !found_keyword && t.kind() == SyntaxKind::WORD {
                    found_keyword = true;
                    emit_token(&t, SemanticTokenType::Keyword, out);
                } else if !found_name && t.kind() == SyntaxKind::WORD {
                    found_name = true;
                    emit_token(&t, SemanticTokenType::Type, out);
                } else {
                    visit_token(&t, out);
                }
            }
        }
    }
}

/// Resolve the semantic token type for a type name.
///
/// NOTE: Stubbed — HIR symbol table lookup removed pending compiler2 reimplementation.
/// Returns a conservative default (Namespace) for unknown types.
fn resolve_type_name(_db: &ProjectDatabase, name: &str) -> SemanticTokenType {
    if matches!(
        name,
        "int" | "float" | "string" | "bool" | "map" | "unknown" | "never"
    ) {
        return SemanticTokenType::Type;
    }
    // Without HIR symbol table, we can't distinguish class/enum/alias.
    // Fall back to Namespace for all user-defined types.
    SemanticTokenType::Namespace
}

/// Visit a `TYPE_EXPR` node, resolving type names to their definitions.
fn visit_type_expr(
    db: &ProjectDatabase,
    file: SourceFile,
    node: &SyntaxNode,
    out: &mut Vec<SemanticToken>,
) {
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, file, &n, out),
            NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::WORD => emit_token(&t, resolve_type_name(db, t.text()), out),
                _ => visit_token(&t, out),
            },
        }
    }
}

/// Visit a `FUNCTION_DEF` node.
///
/// NOTE: HIR/TIR-based expression body resolution is stubbed.
/// Falls back to CST-only classification.
fn visit_function_def(
    db: &ProjectDatabase,
    file: SourceFile,
    node: &SyntaxNode,
    out: &mut Vec<SemanticToken>,
) {
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, file, &n, out),
            NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::WORD => emit_token(&t, SemanticTokenType::Function, out),
                _ => visit_token(&t, out),
            },
        }
    }
}
