//! Syntax node and token kinds.

/// All possible syntax elements in a BAML file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
#[allow(non_camel_case_types)]
pub enum SyntaxKind {
    // ============ Token Kinds (from lexer) ============

    // Keywords
    // Top-level declaration keywords
    KW_CLASS,
    KW_ENUM,
    KW_FUNCTION,
    KW_CLIENT,
    KW_GENERATOR,
    KW_TEST,
    KW_RETRY_POLICY,
    KW_TEMPLATE_STRING,
    KW_TYPE_BUILDER,

    // Control flow keywords
    KW_IF,
    KW_ELSE,
    KW_FOR,
    KW_WHILE,
    KW_LET,
    KW_IN,
    KW_BREAK,
    KW_CONTINUE,
    KW_RETURN,
    KW_MATCH,

    // Other keywords
    KW_WATCH,
    KW_INSTANCEOF,
    KW_ENV,
    KW_DYNAMIC,

    // Literals
    WORD,            // Any word (non-keyword identifier)
    INTEGER_LITERAL, // 123
    FLOAT_LITERAL,   // 123.45

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
    DOT_DOT_DOT,  // ...
    DOT,          // .
    DOLLAR,       // $
    ARROW,        // ->
    FAT_ARROW,    // =>
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
    LINE_COMMENT,   // //...
    BLOCK_COMMENT,  // /* ... */
    HEADER_COMMENT, // //# Header (MDX-style)

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
    GENERATOR_DEF,
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
    /// Field access on a complex expression: `arr[0].field`, `f().method`, `(a + b).field`
    ///
    /// Used when the base is NOT a simple identifier chain. For simple identifier
    /// chains like `user.name.length`, use `PATH_EXPR` instead.
    ///
    /// Structure: `<base_expr> DOT WORD`
    ///
    /// The distinction matters because:
    /// - `PATH_EXPR` can resolve to: local variable + field accesses, enum variant,
    ///   module item, or function reference
    /// - `FIELD_ACCESS_EXPR` is always a field/method access on a computed value
    FIELD_ACCESS_EXPR,
    /// Path expression with one or more dot-separated identifier segments.
    ///
    /// Examples:
    /// - Single segment: `foo`, `MyClass`
    /// - Multi-segment: `user.name`, `baml.HttpMethod.Get`, `Status.Active`
    ///
    /// Structure: `WORD (DOT WORD)*`
    ///
    /// Resolution of what a path refers to happens in THIR:
    /// - `user.name` might be local variable + field access
    /// - `Status.Active` might be an enum variant
    /// - `baml.HttpMethod` might be a module path
    ///
    /// For field access on complex expressions (like `f().field` or `arr[0].field`),
    /// use `FIELD_ACCESS_EXPR` instead.
    PATH_EXPR,
    PAREN_EXPR,
    BLOCK_EXPR,
    IF_EXPR,
    MATCH_EXPR,
    MATCH_ARM,
    MATCH_PATTERN,
    MATCH_GUARD,
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
    SPREAD_ELEMENT, // ...expr in object/array literals
    ARRAY_LITERAL,
    MAP_LITERAL,

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
            SyntaxKind::INTEGER_LITERAL
                | SyntaxKind::FLOAT_LITERAL
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
