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
/// The parser collects all tokens between quotes, preserving enough raw text
/// for later stages to decode escape sequences. An unescaped quote terminates
/// the string; a quote preceded by an odd number of consecutive backslashes
/// stays inside the literal (so `"a\""` is one token, but `"a\\"` terminates
/// at the second quote because the backslash is itself escaped).
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
    #[token("interface")]
    Interface,
    #[token("implements")]
    Implements,
    #[token("implement")]
    Implement,
    #[token("extends")]
    Extends,
    #[token("requires")]
    Requires,
    #[token("function")]
    Function,
    #[token("client")]
    Client,
    /// Deprecated: `generator` blocks moved to `baml.toml`. Still lexed so the
    /// parser can recognize stale blocks and raise a migration diagnostic.
    #[token("generator")]
    Generator,
    #[token("test")]
    Test,
    #[token("testset")]
    TestSet,
    #[token("retry_policy")]
    RetryPolicy,
    #[token("template_string")]
    TemplateString,
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
    #[token("throw")]
    Throw,
    #[token("match")]
    Match,
    #[token("catch")]
    Catch,
    #[token("catch_all")]
    CatchAll,
    #[token("throws")]
    Throws,
    #[token("spawn")]
    Spawn,
    #[token("await")]
    Await,
    #[token("defer")]
    Defer,

    // Other keywords
    #[token("instanceof")]
    Instanceof,
    #[token("is")]
    Is,
    // ============ Identifiers and Literals ============
    /// Any identifier-like word (non-keyword)
    /// Also matches $-prefixed identifiers and `$`-separated names.
    /// and `$`-separated names like `Foo$bar`. A *trailing* `$` is intentionally
    /// rejected so that `${` inside a backtick string (BEP-049 interpolation
    /// marker) doesn't get absorbed into a preceding identifier — e.g.
    /// `before-${x}` must lex as `Word("before-"), Dollar, LBrace, ...`,
    /// not `Word("before-$"), LBrace, ...`.
    #[regex(r"\$[a-zA-Z_][a-zA-Z0-9_]*")]
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_-]*(\$[a-zA-Z_][a-zA-Z0-9_-]*)*")]
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

    /// Backtick symbol - used for interpolated string delimiters (BEP-049)
    /// Parser assembles backtick strings by collecting tokens between matching
    /// runs of N backticks (multi-tick ladder, anchored close).
    /// E.g., `hello ${name}` → Backtick, Word("hello"), ..., Backtick
    #[token("`")]
    Backtick,

    /// Bigint literal: decimal (`42n`), hex (`0xFFn`), octal (`0o755n`), or
    /// binary (`0b1010n`) digits with a trailing `n`, with `_` separators
    /// allowed among the digits. Wins over Integer by maximal munch.
    ///
    /// Prefixed forms deliberately over-accept, exactly like `IntegerLiteral`
    /// below — see that token's comment. Validation happens in
    /// `baml_base::num_lit` when the value is parsed.
    #[regex(r"[0-9][0-9_]*n")]
    #[regex(r"0[xX][0-9a-fA-F_]*n")]
    #[regex(r"0[oO][0-9_]*n")]
    #[regex(r"0[bB][0-9_]*n")]
    BigintLiteral,

    /// Integer literal: decimal (`42`), hex (`0xFF`), octal (`0o755`), or
    /// binary (`0b1010`), with `_` separators allowed among the digits.
    ///
    /// The prefixed forms deliberately over-accept (rustc's design) so that a
    /// malformed literal stays one token with a good span instead of
    /// splitting:
    /// - `0b`/`0o` consume any decimal digits, so `0b123` is a single token
    ///   and "invalid digit for a base 2 literal" can point at the `2`.
    /// - Prefixes also match uppercase (`0X1F`), so "base prefixes are
    ///   lowercase" can suggest the fix.
    /// - A bare prefix (`0x`) is still one token, diagnosed as "no valid
    ///   digits found for number".
    ///
    /// Validation and value parsing live in `baml_base::num_lit`.
    #[regex(r"[0-9][0-9_]*")]
    #[regex(r"0[xX][0-9a-fA-F_]*")]
    #[regex(r"0[oO][0-9_]*")]
    #[regex(r"0[bB][0-9_]*")]
    IntegerLiteral,

    /// Float literal (must come after Integer in regex priority).
    ///
    /// Two patterns feed this token:
    /// - `[0-9]+\.[0-9]+` — plain decimals such as `1.0` or `3.14`.
    /// - `[0-9]+(\.[0-9]+)?[eE][+-]?[0-9]+` — scientific notation with an
    ///   integer or decimal mantissa and a required signed/unsigned integer
    ///   exponent, such as `1e10`, `1E3`, `1e-3`, `2e+5`, or `1.5e-3`.
    ///
    /// Maximal munch ensures `1e10` lexes as a single float rather than the
    /// integer `1` followed by an identifier `e10` (which previously surfaced a
    /// misleading "unresolved name" error). The exponent digits are mandatory,
    /// so `1e` still lexes as `IntegerLiteral` + `Word`.
    ///
    /// `_` separators are allowed among digits (`1_000.5`, `1e1_0`), matching
    /// Rust: the fraction must start with a digit (`1._5` stays member
    /// access) and the exponent must contain at least one digit. One
    /// deliberate divergence from Rust: the exponent must *start* with a
    /// digit (`1e_1` is not a float) — logos miscompiles a leading `_*` loop
    /// in the exponent, breaking backtracking for `1e`/`1e10`, and the form
    /// is vanishingly rare. Underscores are stripped by
    /// `baml_base::num_lit::normalize_float_literal` before the text reaches
    /// `f64::from_str` downstream.
    #[regex(r"[0-9][0-9_]*\.[0-9][0-9_]*")]
    #[regex(r"[0-9][0-9_]*(\.[0-9][0-9_]*)?[eE][+-]?[0-9][0-9_]*")]
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
    #[token("..")]
    DotDot,
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
    #[token("?.")]
    QuestionDot,
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

    // Backslash is used for escaping quotes in strings
    #[token("\\")]
    Backslash,

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
            TokenKind::Interface => "interface",
            TokenKind::Implements => "implements",
            TokenKind::Implement => "implement",
            TokenKind::Extends => "extends",
            TokenKind::Requires => "requires",
            TokenKind::Function => "function",
            TokenKind::Client => "client",
            TokenKind::Generator => "generator",
            TokenKind::Test => "test",
            TokenKind::TestSet => "testset",
            TokenKind::RetryPolicy => "retry_policy",
            TokenKind::TemplateString => "template_string",
            TokenKind::If => "if",
            TokenKind::Else => "else",
            TokenKind::For => "for",
            TokenKind::While => "while",
            TokenKind::Let => "let",
            TokenKind::In => "in",
            TokenKind::Break => "break",
            TokenKind::Continue => "continue",
            TokenKind::Return => "return",
            TokenKind::Throw => "throw",
            TokenKind::Match => "match",
            TokenKind::Catch => "catch",
            TokenKind::CatchAll => "catch_all",
            TokenKind::Throws => "throws",
            TokenKind::Spawn => "spawn",
            TokenKind::Await => "await",
            TokenKind::Defer => "defer",
            TokenKind::Instanceof => "instanceof",
            TokenKind::Is => "is",
            // Identifiers and literals
            TokenKind::Word => "identifier",
            TokenKind::Quote => "'\"'",
            TokenKind::Hash => "'#'",
            TokenKind::Backtick => "'`'",
            TokenKind::BigintLiteral => "bigint",
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
            TokenKind::QuestionDot => "'?.'",
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

            // Backslash
            TokenKind::Backslash => "'\\'",

            // Whitespace
            TokenKind::Whitespace => "whitespace",
            TokenKind::Newline => "newline",

            // Error
            TokenKind::Error => "error",

            // Spread/Ellipsis
            TokenKind::DotDotDot => "'...'",
            TokenKind::DotDot => "'..'",
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

/// Return whether `value` is one complete, non-keyword BAML identifier token.
///
/// Lexer keywords are rejected automatically by their dedicated [`TokenKind`].
/// The additional spellings are contextual parser keywords: they intentionally
/// remain `Word` tokens so the parser can recognize them only in the grammar
/// positions where they are meaningful.
pub fn is_baml_identifier(value: &str) -> bool {
    const CONTEXTUAL_KEYWORDS: &[&str] = &[
        "as",
        "catch_all_panics",
        "const",
        "false",
        "map",
        "null",
        "true",
        "type",
        "unreflect",
        "with",
    ];

    let mut lexer = TokenKind::lexer(value);
    matches!(lexer.next(), Some(Ok(TokenKind::Word)))
        && lexer.next().is_none()
        && !CONTEXTUAL_KEYWORDS.contains(&value)
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
    fn dynamic_is_an_identifier() {
        assert_eq!(lex_no_whitespace("dynamic"), vec![TokenKind::Word]);
    }

    #[test]
    fn test_word_with_dollar() {
        // Dollar-qualified names (e.g. companion functions) tokenize as a single Word
        let source = "ExtractResume$render_prompt Foo$bar";
        let tokens = lex_no_whitespace(source);
        assert_eq!(tokens, vec![TokenKind::Word, TokenKind::Word]);

        let all_tokens = lex(source);
        let words: Vec<&str> = all_tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Word)
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(words, vec!["ExtractResume$render_prompt", "Foo$bar"]);

        // $-prefixed words work with dot access.
        let source2 = "foo.$value";
        let tokens2 = lex_no_whitespace(source2);
        assert_eq!(
            tokens2,
            vec![TokenKind::Word, TokenKind::Dot, TokenKind::Word]
        );
        let all_tokens2 = lex(source2);
        let words2: Vec<&str> = all_tokens2
            .iter()
            .filter(|t| t.kind == TokenKind::Word)
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(words2, vec!["foo", "$value"]);
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
    fn test_raw_string_with_braces() {
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
    fn test_backtick_basic() {
        let source = "`hello`";
        let tokens = lex_no_whitespace(source);

        assert_eq!(
            tokens,
            vec![
                TokenKind::Backtick,
                TokenKind::Word, // hello
                TokenKind::Backtick,
            ]
        );

        assert_eq!(reconstruct_source(&lex(source)), source);
    }

    #[test]
    fn test_backtick_multi_tick_ladder() {
        // Three opening + three closing backticks
        let source = "```contains `single` ticks```";
        let tokens = lex_no_whitespace(source);

        assert_eq!(
            tokens,
            vec![
                TokenKind::Backtick,
                TokenKind::Backtick,
                TokenKind::Backtick,
                TokenKind::Word, // contains
                TokenKind::Backtick,
                TokenKind::Word, // single
                TokenKind::Backtick,
                TokenKind::Word, // ticks
                TokenKind::Backtick,
                TokenKind::Backtick,
                TokenKind::Backtick,
            ]
        );

        assert_eq!(reconstruct_source(&lex(source)), source);
    }

    #[test]
    fn test_backtick_with_interpolation_tokens() {
        // ${...} inside backtick — lexer emits the raw tokens; parser assembles
        let source = "`Hello ${name}`";
        let tokens = lex_no_whitespace(source);

        assert_eq!(
            tokens,
            vec![
                TokenKind::Backtick,
                TokenKind::Word, // Hello
                TokenKind::Dollar,
                TokenKind::LBrace,
                TokenKind::Word, // name
                TokenKind::RBrace,
                TokenKind::Backtick,
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
    fn test_escaped_quote_in_string() {
        // Test that backslashes in strings are not treated as quotes
        let source = r#""This is a \" string""#;
        let tokens = lex_token_kinds(source);

        assert_eq!(
            tokens,
            vec![
                TokenKind::Quote,
                TokenKind::Word, // This
                TokenKind::Whitespace,
                TokenKind::Is, // is (now a keyword, even inside strings — the parser assembles string literals from raw tokens)
                TokenKind::Whitespace,
                TokenKind::Word, // a
                TokenKind::Whitespace,
                TokenKind::Backslash,
                TokenKind::Quote,
                TokenKind::Whitespace,
                TokenKind::Word, // string
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
                TokenKind::Is, // is (keyword; the parser, not the lexer, recognises this as comment content)
                TokenKind::Whitespace, //
                TokenKind::Word, // a
                TokenKind::Whitespace, //
                TokenKind::Word, // comment
                TokenKind::Newline, // \n
                TokenKind::Word, // code
            ]
        );

        // Verify lossless
        assert_eq!(reconstruct_source(&lex(source)), source);
    }

    #[test]
    fn test_keyword_in_identifier() {
        // `get_client` should be a single WORD, not `get_` + keyword `client`
        let tokens = lex_no_whitespace("get_client");
        assert_eq!(tokens, vec![TokenKind::Word]);

        let all = lex("get_client");
        assert_eq!(all[0].text, "get_client");
    }

    #[test]
    fn baml_identifiers_exclude_lexer_and_contextual_keywords() {
        for keyword in [
            "class",
            "test",
            "return",
            "is",
            "as",
            "catch_all_panics",
            "const",
            "false",
            "map",
            "null",
            "true",
            "type",
            "unreflect",
            "with",
        ] {
            assert!(
                !is_baml_identifier(keyword),
                "keyword {keyword:?} must not be accepted as an identifier"
            );
        }

        for identifier in ["Thing", "field_name", "get_client", "$companion", "Foo$bar"] {
            assert!(
                is_baml_identifier(identifier),
                "ordinary spelling {identifier:?} must remain a valid identifier"
            );
        }
    }

    #[test]
    fn test_exception_keywords() {
        let tokens = lex_no_whitespace("throw catch");
        assert_eq!(tokens, vec![TokenKind::Throw, TokenKind::Catch,]);

        // catch_all is a keyword; catch_all_panics lexes as a plain identifier
        let tokens2 = lex_no_whitespace("catch_all catch_all_panics");
        assert_eq!(tokens2, vec![TokenKind::CatchAll, TokenKind::Word,]);
    }

    #[test]
    fn test_path_with_keyword_segment() {
        // `ai.internal.get_client` should be 5 tokens: WORD DOT WORD DOT WORD
        let tokens = lex_no_whitespace("ai.internal.get_client");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Word, // baml
                TokenKind::Dot,
                TokenKind::Word, // llm
                TokenKind::Dot,
                TokenKind::Word, // get_client
            ]
        );
    }

    #[test]
    fn test_optional_chaining_and_null_coalescing() {
        // ?. should be a single token
        let tokens = lex_no_whitespace("a?.b");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Word,        // a
                TokenKind::QuestionDot, // ?.
                TokenKind::Word,        // b
            ]
        );

        // ?? is two Question tokens at the lexer level
        // (parser combines them to avoid ambiguity with int?? double optional)
        let tokens = lex_no_whitespace("a ?? b");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Word,     // a
                TokenKind::Question, // ?
                TokenKind::Question, // ?
                TokenKind::Word,     // b
            ]
        );

        // ? alone (for optional types) should still work
        let tokens = lex_no_whitespace("int?");
        assert_eq!(tokens, vec![TokenKind::Word, TokenKind::Question]);

        // int?? is two Question tokens (double optional)
        let tokens = lex_no_whitespace("int??");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Word,     // int
                TokenKind::Question, // ?
                TokenKind::Question, // ?
            ]
        );

        // Chaining: a?.b?.c ?? d
        let tokens = lex_no_whitespace("a?.b?.c");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Word,        // a
                TokenKind::QuestionDot, // ?.
                TokenKind::Word,        // b
                TokenKind::QuestionDot, // ?.
                TokenKind::Word,        // c
            ]
        );

        // Lossless reconstruction
        let source = "a?.b ?? c";
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

    #[test]
    fn lex_scientific_notation_float() {
        // Scientific notation lexes as a single FloatLiteral token rather than
        // splitting the exponent into a misleading `Word` (the bug this fixes).
        for src in [
            "1e10", "1E3", "1e-3", "2e+5", "1.0e10", "1.5e3", "1.5e-3", "10E-2",
        ] {
            assert_eq!(
                lex_no_whitespace(src),
                vec![TokenKind::FloatLiteral],
                "expected a single FloatLiteral for {src:?}"
            );
            assert_eq!(reconstruct_source(&lex(src)), src);
        }

        // The exponent digits are mandatory: a bare `e` is not consumed.
        assert_eq!(
            lex_no_whitespace("1e"),
            vec![TokenKind::IntegerLiteral, TokenKind::Word]
        );

        // A space breaks scientific notation back into integer + identifier.
        assert_eq!(
            lex_no_whitespace("1 e10"),
            vec![TokenKind::IntegerLiteral, TokenKind::Word]
        );

        // Plain decimals and integers are unaffected (no regression).
        assert_eq!(lex_no_whitespace("3.14"), vec![TokenKind::FloatLiteral]);
        assert_eq!(lex_no_whitespace("42"), vec![TokenKind::IntegerLiteral]);

        // Member access on an integer is still integer + dot + word, because the
        // float patterns require either fractional digits or an exponent.
        assert_eq!(
            lex_no_whitespace("3.foo"),
            vec![TokenKind::IntegerLiteral, TokenKind::Dot, TokenKind::Word]
        );
    }

    #[test]
    fn lex_bigint_literal() {
        // Plain bigint literals
        assert_eq!(lex_no_whitespace("42n"), vec![TokenKind::BigintLiteral]);
        assert_eq!(lex_no_whitespace("0n"), vec![TokenKind::BigintLiteral]);
        assert_eq!(
            lex_no_whitespace("99999999999999999999n"),
            vec![TokenKind::BigintLiteral]
        );

        // Bigint followed by an identifier — must not consume the identifier part
        assert_eq!(
            lex_no_whitespace("42na"),
            vec![TokenKind::BigintLiteral, TokenKind::Word]
        );

        // `42n.5` — the `42n` is consumed as BigintLiteral, leaving `.5`
        // which splits into Dot then IntegerLiteral (not FloatLiteral because the
        // integer part was already consumed).
        assert_eq!(
            lex_no_whitespace("42n.5"),
            vec![
                TokenKind::BigintLiteral,
                TokenKind::Dot,
                TokenKind::IntegerLiteral
            ]
        );

        // Verify that plain integers still lex as IntegerLiteral (no regression)
        assert_eq!(lex_no_whitespace("42"), vec![TokenKind::IntegerLiteral]);

        // Lossless reconstruction
        assert_eq!(reconstruct_source(&lex("42n")), "42n");
        assert_eq!(reconstruct_source(&lex("0n")), "0n");
    }

    #[test]
    fn lex_prefixed_int_literals() {
        // Hex, octal, and binary literals are single IntegerLiteral tokens.
        for src in [
            "0xFF", "0xff", "0xCAFE", "0o755", "0b1010", "0x0", "0o0", "0b0",
        ] {
            assert_eq!(
                lex_no_whitespace(src),
                vec![TokenKind::IntegerLiteral],
                "expected a single IntegerLiteral for {src:?}"
            );
            assert_eq!(reconstruct_source(&lex(src)), src);
        }

        // Uppercase prefixes stay one token so validation can suggest the
        // lowercase spelling instead of the text splitting into `0` + word.
        for src in ["0X1F", "0B10", "0O7"] {
            assert_eq!(
                lex_no_whitespace(src),
                vec![TokenKind::IntegerLiteral],
                "expected a single IntegerLiteral for {src:?}"
            );
        }

        // Binary/octal over-accept decimal digits so invalid literals stay
        // one token; the error is diagnosed at value-parse time.
        for src in ["0b123", "0b10_10301", "0o18", "0o1234_9_5670"] {
            assert_eq!(
                lex_no_whitespace(src),
                vec![TokenKind::IntegerLiteral],
                "expected a single IntegerLiteral for {src:?}"
            );
        }

        // A bare prefix is one token, diagnosed as "no valid digits" later.
        for src in ["0x", "0b", "0o", "0x__", "0b_"] {
            assert_eq!(
                lex_no_whitespace(src),
                vec![TokenKind::IntegerLiteral],
                "expected a single IntegerLiteral for {src:?}"
            );
        }

        // Non-digits for the base are not consumed.
        assert_eq!(
            lex_no_whitespace("0xG"),
            vec![TokenKind::IntegerLiteral, TokenKind::Word]
        );
        assert_eq!(
            lex_no_whitespace("0b1f"),
            vec![TokenKind::IntegerLiteral, TokenKind::Word]
        );

        // Member access and float-like tails do not get absorbed.
        assert_eq!(
            lex_no_whitespace("0xFF.to_string"),
            vec![TokenKind::IntegerLiteral, TokenKind::Dot, TokenKind::Word]
        );
        assert_eq!(
            lex_no_whitespace("0x1.5"),
            vec![
                TokenKind::IntegerLiteral,
                TokenKind::Dot,
                TokenKind::IntegerLiteral
            ]
        );
    }

    #[test]
    fn lex_underscore_separators() {
        // Underscores among digits stay inside a single literal token.
        for (src, kind) in [
            ("1_000", TokenKind::IntegerLiteral),
            ("1_", TokenKind::IntegerLiteral),
            ("1_2_3", TokenKind::IntegerLiteral),
            ("0xFF_FF", TokenKind::IntegerLiteral),
            ("0x_F", TokenKind::IntegerLiteral),
            ("0b10_10", TokenKind::IntegerLiteral),
            ("0o7_5_5", TokenKind::IntegerLiteral),
            ("1_000.000_1", TokenKind::FloatLiteral),
            ("1_.5", TokenKind::FloatLiteral),
            ("1_0e1_0", TokenKind::FloatLiteral),
            ("1e-1_0", TokenKind::FloatLiteral),
            ("1e1_", TokenKind::FloatLiteral),
            ("1_000n", TokenKind::BigintLiteral),
            ("0xFF_FFn", TokenKind::BigintLiteral),
            ("0o755n", TokenKind::BigintLiteral),
            ("0b10_10n", TokenKind::BigintLiteral),
        ] {
            assert_eq!(
                lex_no_whitespace(src),
                vec![kind],
                "expected a single {kind:?} for {src:?}"
            );
            assert_eq!(reconstruct_source(&lex(src)), src);
        }

        // A leading underscore is an identifier, not a literal (as in Rust).
        assert_eq!(lex_no_whitespace("_1"), vec![TokenKind::Word]);

        // The fraction must start with a digit: `1._5` stays member access.
        assert_eq!(
            lex_no_whitespace("1._5"),
            vec![TokenKind::IntegerLiteral, TokenKind::Dot, TokenKind::Word]
        );

        // The exponent must start with a digit (deliberate divergence from
        // Rust, which allows `1e_1` — see the FloatLiteral regex comment).
        assert_eq!(
            lex_no_whitespace("1e_"),
            vec![TokenKind::IntegerLiteral, TokenKind::Word]
        );
        assert_eq!(
            lex_no_whitespace("1e_1"),
            vec![TokenKind::IntegerLiteral, TokenKind::Word]
        );
    }
}
