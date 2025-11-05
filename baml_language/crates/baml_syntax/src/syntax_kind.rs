//! Syntax node and token kinds.

/// All possible syntax elements in a BAML file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
#[allow(non_camel_case_types)]
pub enum SyntaxKind {
    // ============ Token Kinds (from lexer) ============

    // Literals
    WORD,    // Any word (keywords determined by parser)
    INTEGER, // 123
    FLOAT,   // 123.45

    // String delimiters (parser assembles strings)
    QUOTE, // "
    HASH,  // # (for raw strings)

    // Brackets
    L_BRACE,   // {
    R_BRACE,   // }
    L_PAREN,   // (
    R_PAREN,   // )
    L_BRACKET, // [
    R_BRACKET, // ]

    // Punctuation
    COLON,        // :
    DOUBLE_COLON, // ::
    COMMA,        // ,
    SEMICOLON,    // ;
    DOT,          // .
    ARROW,        // ->
    AT,           // @
    AT_AT,        // @@
    PIPE,         // |
    QUESTION,     // ?

    // Assignment operators
    EQUALS,                 // =
    PLUS_EQUALS,            // +=
    MINUS_EQUALS,           // -=
    STAR_EQUALS,            // *=
    SLASH_EQUALS,           // /=
    PERCENT_EQUALS,         // %=
    AND_EQUALS,             // &=
    PIPE_EQUALS,            // |=
    CARET_EQUALS,           // ^=
    LESS_LESS_EQUALS,       // <<=
    GREATER_GREATER_EQUALS, // >>=

    // Comparison operators
    EQUALS_EQUALS,  // ==
    NOT_EQUALS,     // !=
    LESS,           // <
    GREATER,        // >
    LESS_EQUALS,    // <=
    GREATER_EQUALS, // >=

    // Logical operators
    AND_AND, // &&
    OR_OR,   // ||
    NOT,     // !

    // Bitwise operators
    AND,             // &
    CARET,           // ^
    TILDE,           // ~
    LESS_LESS,       // <<
    GREATER_GREATER, // >>

    // Arithmetic operators
    PLUS,        // +
    MINUS,       // -
    STAR,        // *
    SLASH,       // /
    PERCENT,     // %
    PLUS_PLUS,   // ++
    MINUS_MINUS, // --

    // Whitespace and comments (preserved for losslessness)
    WHITESPACE,
    NEWLINE,
    LINE_COMMENT,  // //...
    BLOCK_COMMENT, // /* ... */

    // Error token
    ERROR_TOKEN,

    // ============ Composite Node Kinds ============

    // Root
    SOURCE_FILE,

    // Top-level items
    FUNCTION_DEF,
    CLASS_DEF,
    ENUM_DEF,
    CLIENT_DEF,
    TEST_DEF,
    RETRY_POLICY_DEF,
    TEMPLATE_STRING_DEF,
    TYPE_ALIAS_DEF,

    // Function components
    PARAMETER_LIST,
    PARAMETER,
    FUNCTION_BODY,
    LLM_FUNCTION_BODY,  // Function body with client/prompt
    EXPR_FUNCTION_BODY, // Function body with expressions/statements
    PROMPT_FIELD,
    CLIENT_REFERENCE,
    CLIENT_FIELD, // 'client' field in LLM function
    DEFAULT_IMPL,

    // Class components
    FIELD_LIST,
    FIELD,

    // Enum components
    ENUM_VARIANT_LIST,
    ENUM_VARIANT,

    // Client components
    CLIENT_TYPE, // <llm> part
    CONFIG_BLOCK,
    CONFIG_ITEM,
    CONFIG_VALUE,
    NESTED_CONFIG,

    // Type expressions
    TYPE_EXPR,
    UNION_TYPE,
    OPTIONAL_TYPE,
    ARRAY_TYPE,
    MAP_TYPE,
    TYPE_ARGS,
    STRING_LITERAL_TYPE, // "user" | "assistant"

    // Attributes
    ATTRIBUTE,       // @alias("name")
    BLOCK_ATTRIBUTE, // @@dynamic
    ATTRIBUTE_ARGS,

    // Expressions (for attributes and function bodies)
    EXPR,
    BINARY_EXPR,
    UNARY_EXPR,
    CALL_EXPR,
    INDEX_EXPR,
    FIELD_ACCESS_EXPR,
    PATH_EXPR,
    PAREN_EXPR,
    BLOCK_EXPR,
    IF_EXPR,
    WHILE_STMT,
    FOR_EXPR,
    LET_STMT,
    WATCH_LET,
    BREAK_STMT,
    CONTINUE_STMT,
    RETURN_STMT,

    // Expression components
    CALL_ARGS,
    GENERIC_ARGS,
    OBJECT_LITERAL,
    OBJECT_FIELD,
    ARRAY_LITERAL,

    // String components (assembled by parser)
    STRING_LITERAL,
    RAW_STRING_LITERAL,
    UNQUOTED_STRING,

    // Template components (inside raw strings)
    TEMPLATE_CONTENT,
    TEMPLATE_INTERPOLATION, // {{ expr }}
    TEMPLATE_CONTROL,       // {% for ... %}
    TEMPLATE_COMMENT,       // {# comment #}

    // Error recovery
    ERROR,

    // Placeholder for future extensions
    #[doc(hidden)]
    __LAST,
}

impl SyntaxKind {
    /// Check if this is a trivia token (whitespace, comments).
    pub fn is_trivia(self) -> bool {
        matches!(
            self,
            SyntaxKind::WHITESPACE
                | SyntaxKind::NEWLINE
                | SyntaxKind::LINE_COMMENT
                | SyntaxKind::BLOCK_COMMENT
        )
    }

    /// Check if this is a literal token.
    pub fn is_literal(self) -> bool {
        matches!(
            self,
            SyntaxKind::INTEGER
                | SyntaxKind::FLOAT
                | SyntaxKind::STRING_LITERAL
                | SyntaxKind::RAW_STRING_LITERAL
        )
    }

    /// Check if this is an operator token.
    pub fn is_operator(self) -> bool {
        use SyntaxKind::{
            AND, AND_AND, CARET, EQUALS, EQUALS_EQUALS, GREATER, GREATER_EQUALS, GREATER_GREATER,
            LESS, LESS_EQUALS, LESS_LESS, MINUS, MINUS_EQUALS, NOT, NOT_EQUALS, OR_OR, PERCENT,
            PIPE, PLUS, PLUS_EQUALS, SLASH, SLASH_EQUALS, STAR, STAR_EQUALS, TILDE,
        };
        matches!(
            self,
            PLUS | MINUS
                | STAR
                | SLASH
                | PERCENT
                | EQUALS
                | PLUS_EQUALS
                | MINUS_EQUALS
                | STAR_EQUALS
                | SLASH_EQUALS
                | EQUALS_EQUALS
                | NOT_EQUALS
                | LESS
                | GREATER
                | LESS_EQUALS
                | GREATER_EQUALS
                | AND_AND
                | OR_OR
                | NOT
                | AND
                | PIPE
                | CARET
                | TILDE
                | LESS_LESS
                | GREATER_GREATER
        )
    }
}

// Conversion for Rowan
impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        rowan::SyntaxKind(kind as u16)
    }
}

impl From<rowan::SyntaxKind> for SyntaxKind {
    fn from(raw: rowan::SyntaxKind) -> Self {
        assert!(raw.0 <= SyntaxKind::__LAST as u16);
        #[allow(unsafe_code)]
        unsafe {
            std::mem::transmute(raw.0)
        }
    }
}
