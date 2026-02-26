use baml_db::{
    Name, QualifiedName, SourceFile, baml_compiler_hir, baml_compiler_parser,
    baml_compiler_syntax::{SyntaxKind, SyntaxNode, SyntaxToken, ast},
};
use rowan::{NodeOrToken, ast::AstNode};
use text_size::TextRange;

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

/// Legend order — index in this array is the `token_type` value in the LSP protocol.
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
    pub fn legend_index(self) -> u32 {
        TOKEN_TYPES.iter().position(|t| *t == self).unwrap_or(0) as u32
    }

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

pub fn semantic_tokens(
    db: &dyn baml_db::baml_compiler_hir::Db,
    file: SourceFile,
) -> Vec<SemanticToken> {
    let root = baml_compiler_parser::syntax_tree(db, file);
    let mut out = Vec::new();
    visit_node(db, &root, &mut out);
    out
}

/// Dispatch a single node to its visitor.
fn visit_node(db: &dyn baml_compiler_hir::Db, node: &SyntaxNode, out: &mut Vec<SemanticToken>) {
    match node.kind() {
        SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
            emit_node(node, SemanticTokenType::String, out);
        }
        SyntaxKind::ATTRIBUTE => visit_attribute(db, node, out),
        SyntaxKind::BLOCK_ATTRIBUTE => visit_attribute(db, node, out),
        SyntaxKind::TYPE_ALIAS_DEF => visit_type_alias_def(db, node, out),
        SyntaxKind::ENUM_DEF => visit_word_as(db, node, SemanticTokenType::Enum, out),
        SyntaxKind::ENUM_VARIANT => visit_word_as(db, node, SemanticTokenType::EnumMember, out),
        SyntaxKind::CLASS_DEF => visit_word_as(db, node, SemanticTokenType::Class, out),
        SyntaxKind::FIELD => visit_word_as(db, node, SemanticTokenType::Property, out),
        SyntaxKind::FUNCTION_DEF => visit_word_as(db, node, SemanticTokenType::Function, out),
        SyntaxKind::PARAMETER => visit_word_as(db, node, SemanticTokenType::Parameter, out),
        SyntaxKind::TYPE_EXPR => visit_type_expr(db, node, out),
        SyntaxKind::LET_STMT => visit_first_word_as(db, node, SemanticTokenType::Variable, out),
        SyntaxKind::OBJECT_LITERAL => visit_object_literal(db, node, out),
        SyntaxKind::OBJECT_FIELD => visit_first_word_as(db, node, SemanticTokenType::Property, out),
        _ => visit_children(db, node, out),
    }
}

/// Classify a leaf token into a semantic token type.
fn visit_token(token: &SyntaxToken, out: &mut Vec<SemanticToken>) {
    let kind = token.kind();
    if kind.is_whitespace() {
        return;
    }
    let token_type = if kind.is_keyword() {
        SemanticTokenType::Keyword
    } else if kind.is_operator() {
        SemanticTokenType::Operator
    } else if kind.is_comment() {
        SemanticTokenType::Comment
    } else if matches!(
        kind,
        SyntaxKind::INTEGER_LITERAL | SyntaxKind::FLOAT_LITERAL
    ) {
        SemanticTokenType::Number
    } else {
        return;
    };
    out.push(SemanticToken {
        range: token.text_range(),
        token_type,
    });
}

/// Walk all children of a node, dispatching child nodes via `visit_node`
/// and classifying leaf tokens via `visit_token`.
fn visit_children(db: &dyn baml_compiler_hir::Db, node: &SyntaxNode, out: &mut Vec<SemanticToken>) {
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, &n, out),
            NodeOrToken::Token(t) => visit_token(&t, out),
        }
    }
}

/// Visit a node where all WORD tokens should be classified as `word_type`.
fn visit_word_as(
    db: &dyn baml_compiler_hir::Db,
    node: &SyntaxNode,
    word_type: SemanticTokenType,
    out: &mut Vec<SemanticToken>,
) {
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, &n, out),
            NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::WORD => emit_token(&t, word_type, out),
                _ => visit_token(&t, out),
            },
        }
    }
}

/// Visit a node where the first WORD token should be classified as `word_type`.
fn visit_first_word_as(
    db: &dyn baml_compiler_hir::Db,
    node: &SyntaxNode,
    word_type: SemanticTokenType,
    out: &mut Vec<SemanticToken>,
) {
    let mut found_word = false;
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, &n, out),
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

/// Visit an attribute or a block attribute node.
fn visit_attribute(
    db: &dyn baml_compiler_hir::Db,
    node: &SyntaxNode,
    out: &mut Vec<SemanticToken>,
) {
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, &n, out),
            NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::AT_AT | SyntaxKind::AT | SyntaxKind::WORD => {
                    emit_token(&t, SemanticTokenType::Decorator, out)
                }
                _ => visit_token(&t, out),
            },
        }
    }
}

fn visit_type_alias_def(
    db: &dyn baml_compiler_hir::Db,
    node: &SyntaxNode,
    out: &mut Vec<SemanticToken>,
) {
    let mut found_keyword = false; // "type" is not actually a keyword, it's a WORD token.
    let mut found_name = false;
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, &n, out),
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

/// Resolve the semantic token type for a type name by looking it up in the symbol table.
fn resolve_type_name(db: &dyn baml_compiler_hir::Db, name: &str) -> SemanticTokenType {
    let project = db.project();
    let symbol_table = baml_compiler_hir::symbol_table(db, project);
    let fqn = QualifiedName::local(Name::new(name));
    match symbol_table.lookup_type(db, &fqn) {
        Some(baml_compiler_hir::Definition::Class(_)) => SemanticTokenType::Class,
        Some(baml_compiler_hir::Definition::Enum(_)) => SemanticTokenType::Enum,
        _ => SemanticTokenType::Type,
    }
}

/// Visit a TYPE_EXPR node, resolving type names to their definitions.
fn visit_type_expr(
    db: &dyn baml_compiler_hir::Db,
    node: &SyntaxNode,
    out: &mut Vec<SemanticToken>,
) {
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, &n, out),
            NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::WORD => emit_token(&t, resolve_type_name(db, t.text()), out),
                _ => visit_token(&t, out),
            },
        }
    }
}

fn visit_object_literal(
    db: &dyn baml_compiler_hir::Db,
    node: &SyntaxNode,
    out: &mut Vec<SemanticToken>,
) {
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, &n, out),
            NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::WORD => emit_token(&t, resolve_type_name(db, t.text()), out),
                _ => visit_token(&t, out),
            },
        }
    }
}
