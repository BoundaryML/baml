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
    KW_TESTSET,
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
    KW_THROW,
    KW_MATCH,
    KW_CATCH,
    KW_CATCH_ALL,
    KW_THROWS,
    KW_SPAWN,
    KW_AWAIT,

    // Other keywords
    KW_WATCH,
    KW_INSTANCEOF,
    KW_IS,
    KW_DYNAMIC,
    KW_WITH,

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
    COLON,             // :
    DOUBLE_COLON,      // ::
    COMMA,             // ,
    SEMICOLON,         // ;
    DOT_DOT_DOT,       // ...
    DOT_DOT,           // ..
    DOT,               // .
    DOLLAR,            // $
    ARROW,             // ->
    FAT_ARROW,         // =>
    AT,                // @
    AT_AT,             // @@
    PIPE,              // |
    QUESTION_DOT,      // ?.
    QUESTION_QUESTION, // ??
    QUESTION,          // ?

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

    // Backslash
    BACKSLASH,

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
    TEST_EXPR_DEF,
    TESTSET_DEF,
    RETRY_POLICY_DEF,
    TEMPLATE_STRING_DEF,
    TYPE_ALIAS_DEF,
    TYPE_BUILDER_BLOCK, // type_builder { ... } inside test definitions
    DYNAMIC_TYPE_DEF,   // dynamic class/enum inside type_builder blocks

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
    FUNCTION_TYPE,       // (x: int, y: int) -> int
    FUNCTION_TYPE_PARAM, // x: int (or just int)

    // Attributes
    ATTRIBUTE,       // @alias("name")
    BLOCK_ATTRIBUTE, // @@dynamic
    ATTRIBUTE_ARGS,

    // Expressions (for attributes and function bodies)
    EXPR,
    BINARY_EXPR,
    /// `<expr> is <pattern>` — Rust `matches!`-style pattern test, returns bool.
    ///
    /// Structure: `<expr> KW_IS <PATTERN>`
    IS_EXPR,
    UNARY_EXPR,
    CALL_EXPR,
    INDEX_EXPR,
    /// Optional call: `func?.(args)` — short-circuits to null if callee is null.
    OPTIONAL_CALL_EXPR,
    /// Optional index: `obj?.[expr]` — short-circuits to null if base is null.
    OPTIONAL_INDEX_EXPR,
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
    /// Optional field access: `obj?.field` — short-circuits to null if base is null.
    ///
    /// Structure: `<base_expr> QUESTION_DOT WORD`
    OPTIONAL_FIELD_ACCESS_EXPR,
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
    /// `env.FIELD` expression (e.g., `env.API_KEY`).
    ///
    /// Structure: `WORD("env") DOT WORD`
    ///
    /// Desugared at AST lowering to `baml.env.get_or_panic("FIELD")`.
    ENV_ACCESS_EXPR,
    PAREN_EXPR,
    BLOCK_EXPR,
    IF_EXPR,
    MATCH_EXPR,
    MATCH_ARM,
    MATCH_PATTERN,
    MATCH_GUARD,
    CATCH_EXPR,
    CATCH_CLAUSE,
    CATCH_ARM,
    CATCH_PATTERN,
    CATCH_BINDING,
    CATCH_STACK_TRACE_BINDING,

    // ============ Patterns (unified) ============
    //
    // Used by let-statements, match arms, and catch arms. Grammar:
    //   PATTERN     := CHAIN
    //   CHAIN       := UNION (':' UNION)*
    //   UNION       := ATOM ('|' ATOM)*
    //   ATOM        := BINDING_PATTERN
    //                | DESTRUCTURE_PATTERN
    //                | ARRAY_PATTERN
    //                | TYPE_PATTERN
    //                | PAREN_PATTERN
    //
    // `:` is split before `|`: `let x: int | string` parses as
    // `let x : (int | string)`.
    /// Outer wrapper around any pattern. Always present at recursive entry points.
    PATTERN,
    /// `pat ':' pat (':' pat)*` — type-narrowing chain.
    CHAIN_PATTERN,
    /// `atom ('|' atom)+` — alternation within a single chain link.
    UNION_PATTERN,
    /// `'let' WORD` — introduces a name binding.
    BINDING_PATTERN,
    /// `('let')? PATH '{' field_pattern (',' field_pattern)* '}'` — class destructure.
    DESTRUCTURE_PATTERN,
    /// `WORD` (shorthand) | `WORD ':' PATTERN` (rename / sub-pattern).
    FIELD_PATTERN,
    /// `'[' array_pattern_element (',' array_pattern_element)* ']'`.
    ARRAY_PATTERN,
    /// `PATTERN` or `'..' PATTERN?`.
    ARRAY_PATTERN_ELEMENT,
    /// Bare type expression as a pattern (literals, paths, generics, arrays, …).
    /// Does NOT consume `|` — that belongs to `UNION_PATTERN` at the pattern level.
    TYPE_PATTERN,
    /// `'(' PATTERN ')'` — explicit grouping.
    PAREN_PATTERN,
    /// `'_'` (bare) or `'let' '_'` — wildcard / discard. Distinct from
    /// `BINDING_PATTERN` so downstream code doesn't have to text-match `_`.
    WILDCARD_PATTERN,
    THROW_EXPR,
    /// `spawn name_expr? block` — BEP-034 spawn expression.
    /// Structure: `KW_SPAWN [expr] BLOCK_EXPR`.
    SPAWN_EXPR,
    /// `await expr` — BEP-034 await expression.
    /// Structure: `KW_AWAIT expr`.
    AWAIT_EXPR,
    /// `Future<T, E>` — explicit future type expression.
    /// Structure: `WORD("Future") LESS type_expr COMMA type_expr GREATER`.
    /// Parsed as a generic path type today; this kind exists for the
    /// parser to mark the syntactic origin when the surface form should
    /// resolve to a `Ty::Future`.
    FUTURE_TYPE_EXPR,
    LAMBDA_EXPR,
    THROWS_CLAUSE,
    WHILE_STMT,
    FOR_EXPR,
    LET_STMT,
    WATCH_LET,
    BREAK_STMT,
    CONTINUE_STMT,
    RETURN_STMT,
    THROW_STMT,

    // Expression components
    CALL_ARGS,
    CALL_ARG,
    GENERIC_ARGS,
    /// Declaration-site generic type parameter list: `<T>` or `<K, V>` on class/function defs.
    GENERIC_PARAM_LIST,
    /// A single type parameter name inside a `GENERIC_PARAM_LIST`.
    GENERIC_PARAM,
    OBJECT_LITERAL,
    OBJECT_FIELD,
    SPREAD_ELEMENT, // ...expr in object/array literals
    ARRAY_LITERAL,
    MAP_LITERAL,

    // String components (assembled by parser)
    STRING_LITERAL,
    RAW_STRING_LITERAL,
    BYTE_STRING_LITERAL,
    UNQUOTED_STRING,

    // Template components (inside raw strings)
    TEMPLATE_CONTENT,       // Plain text (deprecated, use PROMPT_TEXT)
    TEMPLATE_INTERPOLATION, // {{ expr }} - Jinja expressions
    TEMPLATE_CONTROL,       // {% for ... %} - Jinja statements
    TEMPLATE_COMMENT,       // {# comment #} - Jinja comments
    PROMPT_TEXT,            // Plain text between Jinja constructs

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

    /// Check if this is a whitespace token.
    pub fn is_whitespace(self) -> bool {
        matches!(self, SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE)
    }

    /// Check if this is a comment token.
    pub fn is_comment(self) -> bool {
        matches!(
            self,
            SyntaxKind::LINE_COMMENT | SyntaxKind::BLOCK_COMMENT | SyntaxKind::HEADER_COMMENT
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
                | SyntaxKind::BYTE_STRING_LITERAL
        )
    }

    /// Check if this is an operator token.
    pub fn is_operator(self) -> bool {
        use SyntaxKind::{
            AND, AND_AND, CARET, EQUALS, EQUALS_EQUALS, GREATER, GREATER_EQUALS, GREATER_GREATER,
            LESS, LESS_EQUALS, LESS_LESS, MINUS, MINUS_EQUALS, NOT, NOT_EQUALS, OR_OR, PERCENT,
            PIPE, PLUS, PLUS_EQUALS, QUESTION_DOT, QUESTION_QUESTION, SLASH, SLASH_EQUALS, STAR,
            STAR_EQUALS, TILDE,
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
                | QUESTION_DOT
                | QUESTION_QUESTION
        )
    }

    /// Check if this is a keyword token.
    pub fn is_keyword(self) -> bool {
        matches!(
            self,
            Self::KW_CLASS
                | Self::KW_ENUM
                | Self::KW_FUNCTION
                | Self::KW_CLIENT
                | Self::KW_GENERATOR
                | Self::KW_TEST
                | Self::KW_TESTSET
                | Self::KW_RETRY_POLICY
                | Self::KW_TEMPLATE_STRING
                | Self::KW_TYPE_BUILDER
                | Self::KW_IF
                | Self::KW_ELSE
                | Self::KW_FOR
                | Self::KW_WHILE
                | Self::KW_LET
                | Self::KW_IN
                | Self::KW_BREAK
                | Self::KW_CONTINUE
                | Self::KW_RETURN
                | Self::KW_THROW
                | Self::KW_MATCH
                | Self::KW_CATCH
                | Self::KW_CATCH_ALL
                | Self::KW_THROWS
                | Self::KW_SPAWN
                | Self::KW_AWAIT
                | Self::KW_WATCH
                | Self::KW_INSTANCEOF
                | Self::KW_DYNAMIC
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
