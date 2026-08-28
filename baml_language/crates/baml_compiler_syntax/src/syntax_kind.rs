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
    KW_INTERFACE,
    KW_IMPLEMENTS,
    KW_IMPLEMENT,
    KW_EXTENDS,
    KW_REQUIRES,
    KW_FUNCTION,
    KW_CLIENT,
    KW_GENERATOR,
    KW_TEST,
    KW_TESTSET,
    KW_RETRY_POLICY,
    KW_TEMPLATE_STRING,

    // Control flow keywords
    KW_IF,
    KW_ELSE,
    KW_FOR,
    KW_WHILE,
    KW_LET,
    KW_CONST,
    KW_IN,
    KW_BREAK,
    KW_CONTINUE,
    KW_RETURN,
    KW_THROW,
    KW_MATCH,
    KW_CATCH,
    KW_CATCH_ALL,
    KW_CATCH_ALL_PANICS,
    KW_THROWS,
    KW_SPAWN,
    KW_AWAIT,
    KW_DEFER,

    // Other keywords
    KW_INSTANCEOF,
    KW_IS,
    KW_WITH,
    // Contextual keywords re-lexed from a `Word` at parse time (no lexer token).
    KW_AS,    // `.as<T>` cast / `(T as I)` / `field as field`
    KW_TYPE,  // associated-type / type-alias `type Name ...`
    KW_TRUE,  // `true` boolean literal
    KW_FALSE, // `false` boolean literal
    KW_NULL,  // `null` literal

    // Literals
    WORD,            // Any word (non-keyword identifier)
    BIGINT_LITERAL,  // 42n
    INTEGER_LITERAL, // 123
    FLOAT_LITERAL,   // 123.45

    // String delimiters (parser assembles strings)
    QUOTE,    // "
    HASH,     // # (for removed hash string recovery)
    BACKTICK, // ` (for BEP-049 interpolated strings)

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
    INTERFACE_DEF,
    CLIENT_DEF,
    GENERATOR_DEF,
    TEST_EXPR_DEF,
    TESTSET_DEF,
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
    TOOLS_FIELD,  // 'tools' field in LLM function (BEP: tools: [fn, ...])
    DEFAULT_IMPL,

    // Class components
    FIELD_LIST,
    FIELD,

    // Interface components
    METHOD_SIG,            // function name(params) -> ReturnType (no body)
    ASSOCIATED_TYPE_DECL,  // type Item [extends Bound] [= Default] in interface/implements
    EXTENDS_CLAUSE,        // reserved legacy node; interfaces use `requires`
    REQUIRES_CLAUSE,       // requires I1, I2
    IMPLEMENTS_BLOCK,      // implements I { ... } inside a class
    IMPLEMENTS_TARGET,     // the interface name (path) in `implements I`
    INTERFACE_FIELD_LINK,  // interface_field as class_field inside `implements`
    IMPLEMENTS_FOR,        // implements I for T { ... } at top level
    IMPLEMENTS_FOR_TARGET, // the `T` in `implements I for T`

    // Enum components
    ENUM_VARIANT_LIST,
    ENUM_VARIANT,

    // Client components
    CLIENT_TYPE, // <llm> part
    /// `client Name = <expr>;` — a named client value declaration (the
    /// single-path replacement for `client<llm>` config blocks). Children:
    /// `KW_CLIENT`, `WORD` (name), `EQUALS`, one expression node/token,
    /// optional `SEMICOLON`.
    CLIENT_VALUE_DEF,
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
    ATTRIBUTE, // @alias("name")
    BLOCK_ATTRIBUTE,
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
    /// Tagged template literal: a tag identifier immediately followed by
    /// a backtick string literal (BEP-049 §10). Structure: tag-expr child
    /// plus `BACKTICK_STRING_LITERAL` child. Lowered to a call where the
    /// body becomes a lambda producing a `TaggedString` value.
    TAGGED_TEMPLATE_EXPR,
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
    /// Explicit interface/static upcast projection: `<expr>.as<T>`.
    UPCAST_EXPR,
    /// Fully-qualified item reference: `(Base as Interface).item`.
    ///
    /// Structure: `L_PAREN TYPE_EXPR KW_AS TYPE_EXPR R_PAREN DOT WORD` — the
    /// value-namespace twin of the associated-type projection the same
    /// spelling denotes in type position, and the only spelling that pins
    /// BOTH halves of the `(Self type, interface, item)` triple. Needed
    /// wherever neither half can be inferred: a method declared by two
    /// implemented interfaces, or one whose `Self` appears only in return
    /// position.
    QUALIFIED_PATH_EXPR,
    /// LLM function spec reference: `MyFunc@spec` (postfix `@spec` on a path).
    ///
    /// Structure: `<PATH_EXPR> AT WORD("spec")`. Lowered as a first-class
    /// projection of the authored function; no companion symbol is created.
    SPEC_EXPR,
    /// LLM function stream projection: `MyFunc@stream`.
    ///
    /// Structure: `<PATH_EXPR> AT WORD("stream")`. PPIR supplies the matching
    /// compiler-private stream function; AST lowering resolves this syntax to
    /// that ordinary function entry.
    STREAM_EXPR,
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
    /// `if let PATTERN = SCRUTINEE { THEN } (else (BLOCK | IF_EXPR | IF_LET_EXPR))?`
    ///
    /// Refutable pattern match in a condition position. Pattern bindings are
    /// in scope inside `THEN` only — not in `else` or after the `if let`.
    /// Children, in order: `PATTERN`, scrutinee expr, then `BLOCK_EXPR`,
    /// optional else `BLOCK_EXPR` / `IF_EXPR` / `IF_LET_EXPR`.
    IF_LET_EXPR,
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
    /// `return expr?` in expression position — a diverging expression of type
    /// `never` (mirrors `THROW_EXPR`). Lets `return` appear as a `catch`/`match`
    /// arm value (e.g. `_ => return 0`) without the statement-only restriction.
    /// Statement-position `return` still parses as `RETURN_STMT`.
    RETURN_EXPR,
    /// `break` in expression position — a diverging expression of type `never`
    /// (mirrors `RETURN_EXPR`). Lets `break` appear as a `catch`/`match` arm
    /// value (e.g. `0 => break`) without the statement-only restriction.
    /// Statement-position `break` still parses as `BREAK_STMT`.
    BREAK_EXPR,
    /// `continue` in expression position — a diverging expression of type
    /// `never` (mirrors `RETURN_EXPR`). Lets `continue` appear as a
    /// `catch`/`match` arm value (e.g. `0 => continue`) without the
    /// statement-only restriction. Statement-position `continue` still parses
    /// as `CONTINUE_STMT`.
    CONTINUE_EXPR,
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
    /// `while let PATTERN = SCRUTINEE { BODY }`
    ///
    /// Loops while the refutable `pattern` matches `scrutinee`, exiting when
    /// it fails to match. Pattern bindings are in scope inside `BODY` only and
    /// are rebound each iteration. Produces unit and has no `else` clause
    /// (unlike `IF_LET_EXPR`). Children, in order: `PATTERN`, scrutinee expr,
    /// then `BLOCK_EXPR`.
    WHILE_LET_STMT,
    FOR_EXPR,
    LET_STMT,
    /// Runtime type binding: `type T = unreflect(expr)`.
    TYPE_BINDING_STMT,
    BREAK_STMT,
    CONTINUE_STMT,
    RETURN_STMT,
    THROW_STMT,
    /// `defer { BODY }` — BEP-042. Runs BODY on every exit of the enclosing
    /// block. Structure: `KW_DEFER BLOCK_EXPR`.
    DEFER_STMT,

    // Expression components
    CALL_ARGS,
    CALL_ARG,
    GENERIC_ARGS,
    /// Contextual runtime type atom: `unreflect(expr)`.
    UNREFLECT_TYPE,
    /// Declaration-site generic type parameter list: `<T>` or `<K, V>` on class/function defs.
    GENERIC_PARAM_LIST,
    /// A single type parameter name inside a `GENERIC_PARAM_LIST`.
    GENERIC_PARAM,
    /// BEP-044: optional bounds on a generic parameter, e.g. `T extends
    /// Iface` or `T extends A & B`. Holds one or more `TYPE_EXPR`
    /// children — multiple entries form an intersection bound.
    GENERIC_PARAM_BOUNDS,
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

    // Backtick interpolated string (BEP-049)
    BACKTICK_STRING_LITERAL, // `...` or ``...`` etc.
    BACKTICK_TEXT,           // Plain text segment between interpolations
    BACKTICK_INTERPOLATION,  // ${ expr } inside a backtick string

    // BEP-049 §5 block-tag forms inside `${...}`. The parser emits these as
    // flat siblings of BACKTICK_INTERPOLATION inside a BACKTICK_STRING_LITERAL;
    // segments() lifts matched open/close pairs into hierarchical For/If
    // structures.
    BACKTICK_FOR_OPEN, // ${for (let x in xs)}
    BACKTICK_ENDFOR,   // ${endfor}
    BACKTICK_IF_OPEN,  // ${if (cond)}
    BACKTICK_ELSE_IF,  // ${else if (cond)}
    BACKTICK_ELSE,     // ${else}
    BACKTICK_ENDIF,    // ${endif}

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
                | Self::KW_INTERFACE
                | Self::KW_IMPLEMENTS
                | Self::KW_IMPLEMENT
                | Self::KW_EXTENDS
                | Self::KW_REQUIRES
                | Self::KW_FUNCTION
                | Self::KW_CLIENT
                | Self::KW_GENERATOR
                | Self::KW_TEST
                | Self::KW_TESTSET
                | Self::KW_RETRY_POLICY
                | Self::KW_TEMPLATE_STRING
                | Self::KW_IF
                | Self::KW_ELSE
                | Self::KW_FOR
                | Self::KW_WHILE
                | Self::KW_LET
                | Self::KW_CONST
                | Self::KW_IN
                | Self::KW_IS
                | Self::KW_BREAK
                | Self::KW_CONTINUE
                | Self::KW_RETURN
                | Self::KW_THROW
                | Self::KW_MATCH
                | Self::KW_CATCH
                | Self::KW_CATCH_ALL
                | Self::KW_CATCH_ALL_PANICS
                | Self::KW_THROWS
                | Self::KW_SPAWN
                | Self::KW_AWAIT
                | Self::KW_DEFER
                | Self::KW_INSTANCEOF
                | Self::KW_WITH
                | Self::KW_AS
                | Self::KW_TYPE
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
