//! Syntax node and token kinds.

/// All possible syntax kinds in BAML.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
#[allow(non_camel_case_types)]
pub enum SyntaxKind {
    // Tokens (from lexer)
    FUNCTION_KW,
    CLASS_KW,
    ENUM_KW,
    CLIENT_KW,
    RETRY_POLICY_KW,
    TEST_KW,
    GENERATOR_KW,

    IDENTIFIER,
    STRING,
    INTEGER,
    FLOAT,

    L_BRACE,
    R_BRACE,
    L_PAREN,
    R_PAREN,
    L_BRACKET,
    R_BRACKET,
    COMMA,
    COLON,
    DOUBLE_COLON,
    ARROW,
    AT,
    AT_AT,
    PIPE,
    QUESTION,
    EQUALS,

    WHITESPACE,
    NEWLINE,
    LINE_COMMENT,
    BLOCK_COMMENT,
    ERROR_TOKEN,

    // Composite nodes (non-terminals)
    ROOT,
    FUNCTION_DEF,
    CLASS_DEF,
    ENUM_DEF,
    CLIENT_DEF,
    RETRY_POLICY_DEF,
    TEST_DEF,
    GENERATOR_DEF,

    PARAMETER_LIST,
    PARAMETER,
    TYPE_EXPR,
    BLOCK,
    FIELD,
    FIELD_LIST,
    ENUM_VALUE,
    ENUM_VALUE_LIST,
    ATTRIBUTE,
    ATTRIBUTE_LIST,

    // Placeholder for error recovery
    ERROR_NODE,
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        rowan::SyntaxKind(kind as u16)
    }
}
