//! Token definitions and lexing implementation.

use baml_base::{FileId, Span};
use logos::Logos;
use text_size::{TextRange, TextSize};

/// Token kinds for BAML.
///
/// The lexer recognizes keywords as distinct tokens per the BAML specification.
///
/// # Note on Unquoted Strings and Raw Strings
///
/// **Unquoted Strings**: BAML supports unquoted strings in config contexts like:
/// ```baml
/// model gpt-4o
/// strategy exponential_backoff
/// ```
/// The lexer tokenizes normally: `"gpt-4o"` → `WORD("gpt"), MINUS, INTEGER("4"), WORD("o")`
/// The parser assembles these into unquoted strings in appropriate contexts.
///
/// **Raw Strings**: Raw strings like `#"..."#` and `##"..."##` are assembled by the parser:
/// ```baml
/// #"Hello {{name}}"#  → Hash, Quote, Word("Hello"), ..., Quote, Hash
/// ##"Contains "#""##  → Hash, Hash, Quote, Word("Contains"), ..., Quote, Hash, Hash
/// ```
/// The parser collects all tokens between `Hash+ Quote` and `Quote Hash+` and validates matching
/// delimiter counts. This provides better error recovery for unclosed raw strings.
///
/// **Regular Strings**: Regular strings are also assembled by the parser:
/// ```baml
/// "hello world"  → Quote, Word("hello"), Word("world"), Quote
/// ```
/// The parser collects all tokens between quotes and handles escape sequences.
///
/// This keeps the lexer simple, context-free, and fast.
#[derive(Logos, Debug, PartialEq, Eq, Clone, Copy)]
pub enum TokenKind {
    // ============ Keywords ============
    // Top-level declaration keywords
    #[token("class")]
    Class,
    #[token("enum")]
    Enum,
    #[token("function")]
    Function,
    #[token("client")]
    Client,
    #[token("generator")]
    Generator,
    #[token("test")]
    Test,
    #[token("retry_policy")]
    RetryPolicy,
    #[token("template_string")]
    TemplateString,
    #[token("type_builder")]
    TypeBuilder,

    // Control flow keywords
    #[token("if")]
    If,
    #[token("else")]
    Else,
    #[token("for")]
    For,
    #[token("while")]
    While,
    #[token("let")]
    Let,
    #[token("in")]
    In,
    #[token("break")]
    Break,
    #[token("continue")]
    Continue,
    #[token("return")]
    Return,
    #[token("match")]
    Match,

    // Other keywords
    #[token("watch")]
    Watch,
    #[token("instanceof")]
    Instanceof,
    #[token("env")]
    Env,
    #[token("dynamic")]
    Dynamic,

    // ============ Identifiers and Literals ============
    /// Any identifier-like word (non-keyword)
    /// Also matches $-prefixed identifiers like $watch for special builtin methods
    #[regex(r"\$[a-zA-Z_][a-zA-Z0-9_]*")]
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_-]*")]
    Word,

    /// Quote symbol - used for string delimiters
    /// Parser assembles strings by collecting tokens between quotes
    /// E.g., "hello" → Quote, Word("hello"), Quote
    #[token("\"")]
    Quote,

    /// Hash symbol - used for raw string delimiters
    /// Parser combines Hash + Quote + tokens + Quote + Hash to form raw strings
    /// E.g., #"hello"# → Hash, Quote, Word("hello"), Quote, Hash
    #[token("#")]
    Hash,

    /// Integer literal
    #[regex(r"[0-9]+")]
    IntegerLiteral,

    /// Float literal (must come after Integer in regex priority)
    #[regex(r"[0-9]+\.[0-9]+")]
    FloatLiteral,

    // ============ Operators and Punctuation ============

    // Brackets
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,

    // Basic punctuation
    #[token("::")]
    DoubleColon,
    #[token(":")]
    Colon,
    #[token(",")]
    Comma,
    #[token(";")]
    Semicolon,
    #[token("...")]
    DotDotDot,
    #[token(".")]
    Dot,
    #[token("$")]
    Dollar,

    // Operators (order matters! Longer tokens first)
    #[token("->")]
    Arrow,
    #[token("=>")]
    FatArrow,
    #[token("@@")]
    AtAt,
    #[token("@")]
    At,
    #[token("|")]
    Pipe,
    #[token("?")]
    Question,

    // Assignment operators (order matters! Longer first)
    #[token("<<=")]
    LessLessEquals,
    #[token(">>=")]
    GreaterGreaterEquals,
    #[token("+=")]
    PlusEquals,
    #[token("-=")]
    MinusEquals,
    #[token("*=")]
    StarEquals,
    #[token("/=")]
    SlashEquals,
    #[token("%=")]
    PercentEquals,
    #[token("&=")]
    AndEquals,
    #[token("|=")]
    PipeEquals,
    #[token("^=")]
    CaretEquals,
    #[token("=")]
    Equals,

    // Comparison operators (order matters! Longer first)
    #[token("==")]
    EqualsEquals,
    #[token("!=")]
    NotEquals,
    #[token("<=")]
    LessEquals,
    #[token(">=")]
    GreaterEquals,
    #[token("<<")]
    LessLess,
    #[token(">>")]
    GreaterGreater,
    #[token("<")]
    Less,
    #[token(">")]
    Greater,

    // Logical operators (order matters! Longer first)
    #[token("&&")]
    AndAnd,
    #[token("||")]
    OrOr,
    #[token("!")]
    Not,

    // Bitwise operators
    #[token("&")]
    And,
    #[token("^")]
    Caret,
    #[token("~")]
    Tilde,

    // Arithmetic operators (order matters! Longer first)
    #[token("++")]
    PlusPlus,
    #[token("--")]
    MinusMinus,
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("%")]
    Percent,

    // ============ Whitespace (preserved for losslessness) ============
    #[regex(r"[ \t]+")]
    Whitespace,

    #[regex(r"\r?\n")]
    Newline,

    // ============ Error token for unrecognized input ============
    Error,
}

impl std::fmt::Display for TokenKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            // Keywords
            TokenKind::Class => "class",
            TokenKind::Enum => "enum",
            TokenKind::Function => "function",
            TokenKind::Client => "client",
            TokenKind::Generator => "generator",
            TokenKind::Test => "test",
            TokenKind::RetryPolicy => "retry_policy",
            TokenKind::TemplateString => "template_string",
            TokenKind::TypeBuilder => "type_builder",
            TokenKind::If => "if",
            TokenKind::Else => "else",
            TokenKind::For => "for",
            TokenKind::While => "while",
            TokenKind::Let => "let",
            TokenKind::In => "in",
            TokenKind::Break => "break",
            TokenKind::Continue => "continue",
            TokenKind::Return => "return",
            TokenKind::Match => "match",
            TokenKind::Watch => "watch",
            TokenKind::Instanceof => "instanceof",
            TokenKind::Env => "env",
            TokenKind::Dynamic => "dynamic",

            // Identifiers and literals
            TokenKind::Word => "identifier",
            TokenKind::Quote => "'\"'",
            TokenKind::Hash => "'#'",
            TokenKind::IntegerLiteral => "integer",
            TokenKind::FloatLiteral => "float",

            // Brackets
            TokenKind::LBrace => "'{'",
            TokenKind::RBrace => "'}'",
            TokenKind::LParen => "'('",
            TokenKind::RParen => "')'",
            TokenKind::LBracket => "'['",
            TokenKind::RBracket => "']'",

            // Punctuation
            TokenKind::DoubleColon => "'::'",
            TokenKind::Colon => "':'",
            TokenKind::Comma => "','",
            TokenKind::Semicolon => "';'",
            TokenKind::Dot => "'.'",
            TokenKind::Dollar => "'$'",

            // Operators
            TokenKind::Arrow => "'->'",
            TokenKind::FatArrow => "'=>'",
            TokenKind::AtAt => "'@@'",
            TokenKind::At => "'@'",
            TokenKind::Pipe => "'|'",
            TokenKind::Question => "'?'",

            // Assignment operators
            TokenKind::LessLessEquals => "'<<='",
            TokenKind::GreaterGreaterEquals => "'>>='",
            TokenKind::PlusEquals => "'+='",
            TokenKind::MinusEquals => "'-='",
            TokenKind::StarEquals => "'*='",
            TokenKind::SlashEquals => "'/='",
            TokenKind::PercentEquals => "'%='",
            TokenKind::AndEquals => "'&='",
            TokenKind::PipeEquals => "'|='",
            TokenKind::CaretEquals => "'^='",
            TokenKind::Equals => "'='",

            // Comparison operators
            TokenKind::EqualsEquals => "'=='",
            TokenKind::NotEquals => "'!='",
            TokenKind::LessEquals => "'<='",
            TokenKind::GreaterEquals => "'>='",
            TokenKind::LessLess => "'<<'",
            TokenKind::GreaterGreater => "'>>'",
            TokenKind::Less => "'<'",
            TokenKind::Greater => "'>'",

            // Logical operators
            TokenKind::AndAnd => "'&&'",
            TokenKind::OrOr => "'||'",
            TokenKind::Not => "'!'",

            // Bitwise operators
            TokenKind::And => "'&'",
            TokenKind::Caret => "'^'",
            TokenKind::Tilde => "'~'",

            // Arithmetic operators
            TokenKind::PlusPlus => "'++'",
            TokenKind::MinusMinus => "'--'",
            TokenKind::Plus => "'+'",
            TokenKind::Minus => "'-'",
            TokenKind::Star => "'*'",
            TokenKind::Slash => "'/'",
            TokenKind::Percent => "'%'",

            // Whitespace
            TokenKind::Whitespace => "whitespace",
            TokenKind::Newline => "newline",

            // Error
            TokenKind::Error => "error",

            // Spread/Ellipsis
            TokenKind::DotDotDot => "'...'",
        };
        write!(f, "{s}")
    }
}

/// A token with its source text and location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub span: Span,
}

/// Lossless lexer that preserves all source text.
///
/// This tokenizes the entire input including whitespace and comments,
/// allowing perfect source reconstruction.
pub fn lex_lossless(input: &str, file_id: FileId) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut lexer = TokenKind::lexer(input);

    while let Some(result) = lexer.next() {
        let kind = result.unwrap_or(TokenKind::Error);
        let span = lexer.span();
        let text = lexer.slice().to_string();

        tokens.push(Token {
            kind,
            text,
            span: Span::new(
                file_id,
                TextRange::new(
                    TextSize::from(u32::try_from(span.start).expect("span.start is too large")),
                    TextSize::from(u32::try_from(span.end).expect("span.end is too large")),
                ),
            ),
        });
    }

    tokens
}

/// Reconstruct source from tokens (for testing losslessness).
pub fn reconstruct_source(tokens: &[Token]) -> String {
    tokens.iter().map(|t| t.text.as_str()).collect()
}

#[cfg(test)]
mod tests {
    use baml_base::FileId;

    use super::*;

    fn lex(source: &str) -> Vec<Token> {
        lex_lossless(source, FileId::new(0))
    }

    fn lex_token_kinds(source: &str) -> Vec<TokenKind> {
        lex(source).iter().map(|t| t.kind).collect()
    }

    fn lex_no_whitespace(source: &str) -> Vec<TokenKind> {
        lex(source)
            .iter()
            .filter(|t| t.kind != TokenKind::Whitespace)
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn test_lossless_lexing() {
        let source = "function test() {}";
        let file_id = FileId::new(0);
        let tokens = lex_lossless(source, file_id);
        let reconstructed = reconstruct_source(&tokens);
        assert_eq!(source, reconstructed);
    }

    #[test]
    fn test_operators() {
        let tokens = lex_no_whitespace("-> :: += -= == != <= >= && ||");

        assert_eq!(
            tokens,
            vec![
                TokenKind::Arrow,
                TokenKind::DoubleColon,
                TokenKind::PlusEquals,
                TokenKind::MinusEquals,
                TokenKind::EqualsEquals,
                TokenKind::NotEquals,
                TokenKind::LessEquals,
                TokenKind::GreaterEquals,
                TokenKind::AndAnd,
                TokenKind::OrOr,
            ]
        );
    }

    #[test]
    fn test_word_with_hyphens() {
        // Words can contain hyphens (e.g., "gpt-4o", "exponential_backoff")
        let source = "gpt-4o model-name";
        let tokens = lex_no_whitespace(source);

        // Should tokenize as: WORD("gpt"), MINUS, INTEGER("4"), WORD("o"), WORD("model"), MINUS, WORD("name")
        // Wait, no - the regex is [a-zA-Z_][a-zA-Z0-9_-]* so hyphens inside words should work
        assert_eq!(tokens, vec![TokenKind::Word, TokenKind::Word]);

        let all_tokens = lex(source);
        let words: Vec<&str> = all_tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Word)
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(words, vec!["gpt-4o", "model-name"]);
    }

    #[test]
    fn test_arithmetic_operators() {
        let tokens = lex_no_whitespace("+ - * / % ++ -- += -= *= /= %=");

        assert_eq!(
            tokens,
            vec![
                TokenKind::Plus,
                TokenKind::Minus,
                TokenKind::Star,
                TokenKind::Slash,
                TokenKind::Percent,
                TokenKind::PlusPlus,
                TokenKind::MinusMinus,
                TokenKind::PlusEquals,
                TokenKind::MinusEquals,
                TokenKind::StarEquals,
                TokenKind::SlashEquals,
                TokenKind::PercentEquals,
            ]
        );
    }

    #[test]
    fn test_bitwise_operators() {
        let tokens = lex_no_whitespace("& | ^ ~ && || &= |= ^=");

        assert_eq!(
            tokens,
            vec![
                TokenKind::And,
                TokenKind::Pipe,
                TokenKind::Caret,
                TokenKind::Tilde,
                TokenKind::AndAnd,
                TokenKind::OrOr,
                TokenKind::AndEquals,
                TokenKind::PipeEquals,
                TokenKind::CaretEquals,
            ]
        );
    }

    #[test]
    fn test_shift_operators() {
        let tokens = lex_no_whitespace("<< >> <<= >>=");

        assert_eq!(
            tokens,
            vec![
                TokenKind::LessLess,
                TokenKind::GreaterGreater,
                TokenKind::LessLessEquals,
                TokenKind::GreaterGreaterEquals,
            ]
        );
    }

    #[test]
    fn test_operator_precedence() {
        // Test that longer operators are matched first
        let source = "<<=";
        let all_tokens = lex(source);

        assert_eq!(all_tokens.len(), 1);
        assert_eq!(all_tokens[0].kind, TokenKind::LessLessEquals);

        // Test >> vs >=
        let tokens = lex_no_whitespace(">>= >= >>");

        assert_eq!(
            tokens,
            vec![
                TokenKind::GreaterGreaterEquals,
                TokenKind::GreaterEquals,
                TokenKind::GreaterGreater,
            ]
        );
    }

    #[test]
    fn test_raw_string_basic() {
        let source = r##"#"Hello World"#"##;
        let tokens = lex_no_whitespace(source);

        // Should lex as: Hash, Quote, Word("Hello"), Word("World"), Quote, Hash
        assert_eq!(
            tokens,
            vec![
                TokenKind::Hash,
                TokenKind::Quote,
                TokenKind::Word,
                TokenKind::Word,
                TokenKind::Quote,
                TokenKind::Hash,
            ]
        );

        // Lossless
        assert_eq!(reconstruct_source(&lex(source)), source);
    }

    #[test]
    fn test_raw_string_multiple_hashes() {
        // With Quote tokens, quotes inside are just more tokens
        let source = r###"##"String with quotes inside"##"###;
        let tokens = lex_no_whitespace(source);

        // Hash, Hash, Quote, ...(words)..., Quote, Hash, Hash
        assert_eq!(
            tokens,
            vec![
                TokenKind::Hash,
                TokenKind::Hash,
                TokenKind::Quote,
                TokenKind::Word, // String
                TokenKind::Word, // with
                TokenKind::Word, // quotes
                TokenKind::Word, // inside
                TokenKind::Quote,
                TokenKind::Hash,
                TokenKind::Hash,
            ]
        );

        assert_eq!(reconstruct_source(&lex(source)), source);
    }

    #[test]
    fn test_raw_string_with_jinja() {
        let source = r##"#"Hello {{ name }}"#"##;
        let tokens = lex_no_whitespace(source);

        assert_eq!(
            tokens,
            vec![
                TokenKind::Hash,
                TokenKind::Quote,
                TokenKind::Word,   // Hello
                TokenKind::LBrace, // {
                TokenKind::LBrace, // {
                TokenKind::Word,   // name
                TokenKind::RBrace, // }
                TokenKind::RBrace, // }
                TokenKind::Quote,
                TokenKind::Hash,
            ]
        );

        assert_eq!(reconstruct_source(&lex(source)), source);
    }

    #[test]
    fn test_raw_string_unclosed() {
        // Unclosed raw string - lexer just emits Hash, Quote, and words
        // Parser will detect the error
        let source = r##"#"Unclosed"##;
        let file_id = FileId::new(0);
        let tokens = lex_lossless(source, file_id);

        // Starts with Hash, Quote
        assert_eq!(tokens[0].kind, TokenKind::Hash);
        assert_eq!(tokens[0].text, "#");
        assert_eq!(tokens[1].kind, TokenKind::Quote);
        assert_eq!(tokens[1].text, "\"");
        // Then Word, then rest of source as unrecognized

        assert_eq!(reconstruct_source(&tokens), source);
    }

    #[test]
    fn test_raw_string_in_context() {
        let source = r##"prompt #"Hello {{ name }}"#"##;
        let tokens = lex_no_whitespace(source);

        assert_eq!(
            tokens,
            vec![
                TokenKind::Word, // prompt
                TokenKind::Hash,
                TokenKind::Quote,
                TokenKind::Word,   // Hello
                TokenKind::LBrace, // {
                TokenKind::LBrace, // {
                TokenKind::Word,   // name
                TokenKind::RBrace, // }
                TokenKind::RBrace, // }
                TokenKind::Quote,
                TokenKind::Hash,
            ]
        );

        // Lossless
        assert_eq!(reconstruct_source(&lex(source)), source);
    }

    #[test]
    fn test_multiple_raw_strings() {
        let source = r##"#"First"# #"Second"#"##;
        let tokens = lex_no_whitespace(source);

        // Should be: Hash, Quote, Word, Quote, Hash, Hash, Quote, Word, Quote, Hash
        assert_eq!(
            tokens,
            vec![
                TokenKind::Hash,
                TokenKind::Quote,
                TokenKind::Word, // First
                TokenKind::Quote,
                TokenKind::Hash,
                TokenKind::Hash,
                TokenKind::Quote,
                TokenKind::Word, // Second
                TokenKind::Quote,
                TokenKind::Hash,
            ]
        );

        // Lossless
        assert_eq!(reconstruct_source(&lex(source)), source);
    }

    #[test]
    fn test_five_hash_delimiter() {
        let source = r######"#####"Complex content here"#####"######;
        let tokens = lex_no_whitespace(source);

        // Should be: 5 Hash, Quote, ...(words)..., Quote, 5 Hash
        assert_eq!(
            tokens,
            vec![
                TokenKind::Hash,
                TokenKind::Hash,
                TokenKind::Hash,
                TokenKind::Hash,
                TokenKind::Hash,
                TokenKind::Quote,
                TokenKind::Word, // Complex
                TokenKind::Word, // content
                TokenKind::Word, // here
                TokenKind::Quote,
                TokenKind::Hash,
                TokenKind::Hash,
                TokenKind::Hash,
                TokenKind::Hash,
                TokenKind::Hash,
            ]
        );
        assert_eq!(reconstruct_source(&lex(source)), source);
    }

    #[test]
    fn test_url_in_string() {
        // Test that URLs with // inside strings are not treated as comments
        let source = r#""https://google.com""#;
        let tokens = lex_token_kinds(source);

        // Should be: Quote, Word("https"), Colon, Slash, Slash, Word("google"), Dot, Word("com"), Quote
        // NOT: Quote, Word("https"), Colon, LineComment
        assert_eq!(
            tokens,
            vec![
                TokenKind::Quote,
                TokenKind::Word, // https
                TokenKind::Colon,
                TokenKind::Slash, // First slash
                TokenKind::Slash, // Second slash (NOT LineComment!)
                TokenKind::Word,  // google
                TokenKind::Dot,
                TokenKind::Word, // com
                TokenKind::Quote,
            ]
        );

        // Verify lossless
        assert_eq!(reconstruct_source(&lex(source)), source);
    }

    #[test]
    fn test_line_comment() {
        // Test that actual line comments (outside strings) are lexed as individual tokens
        let source = "// This is a comment\ncode";
        let tokens = lex_token_kinds(source);

        // Should be: Slash, Slash, Whitespace, Word("This"), ..., Newline, Word("code")
        // The parser will recognize Slash Slash as a comment pattern
        assert_eq!(
            tokens,
            vec![
                TokenKind::Slash,      // /
                TokenKind::Slash,      // /
                TokenKind::Whitespace, //
                TokenKind::Word,       // This
                TokenKind::Whitespace, //
                TokenKind::Word,       // is
                TokenKind::Whitespace, //
                TokenKind::Word,       // a
                TokenKind::Whitespace, //
                TokenKind::Word,       // comment
                TokenKind::Newline,    // \n
                TokenKind::Word,       // code
            ]
        );

        // Verify lossless
        assert_eq!(reconstruct_source(&lex(source)), source);
    }

    #[test]
    fn test_block_comment() {
        // Test that block comments are lexed as individual tokens
        let source = "/* block comment */ code";
        let tokens = lex_token_kinds(source);

        // Should be: Slash, Star, ..., Star, Slash, Whitespace, Word("code")
        // The parser will recognize Slash Star as block comment start
        assert_eq!(
            tokens,
            vec![
                TokenKind::Slash,      // /
                TokenKind::Star,       // *
                TokenKind::Whitespace, //
                TokenKind::Word,       // block
                TokenKind::Whitespace, //
                TokenKind::Word,       // comment
                TokenKind::Whitespace, //
                TokenKind::Star,       // *
                TokenKind::Slash,      // /
                TokenKind::Whitespace, //
                TokenKind::Word,       // code
            ]
        );

        // Verify lossless
        assert_eq!(reconstruct_source(&lex(source)), source);
    }
}
