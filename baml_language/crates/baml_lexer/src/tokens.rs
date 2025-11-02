//! Token definitions and lexing implementation.

use baml_base::{FileId, Span};
use logos::Logos;
use text_size::{TextRange, TextSize};

/// Token kinds for BAML.
///
/// The lexer produces structural tokens only - it does not detect keywords.
/// The parser is responsible for checking if a Word token is a keyword.
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
    // ============ Identifiers and Literals ============
    /// Any identifier-like word (includes what were keywords!)
    /// Parser will check text to determine if it's a keyword
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
    Integer,

    /// Float literal (must come after Integer in regex priority)
    #[regex(r"[0-9]+\.[0-9]+")]
    Float,

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
    #[token(".")]
    Dot,

    // Operators (order matters! Longer tokens first)
    #[token("->")]
    Arrow,
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

    // ============ Comments (for lossless lexing) ============
    #[regex(r"//[^\n]*")]
    LineComment,

    #[regex(r"/\*([^*]|\*[^/])*\*/")]
    BlockComment,

    // ============ Whitespace (preserved for losslessness) ============
    #[regex(r"[ \t]+")]
    Whitespace,

    #[regex(r"\r?\n")]
    Newline,

    // ============ Error token for unrecognized input ============
    Error,
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

    #[test]
    fn test_lossless_lexing() {
        let source = "function test() {}";
        let file_id = FileId::new(0);
        let tokens = lex_lossless(source, file_id);
        let reconstructed = reconstruct_source(&tokens);
        assert_eq!(source, reconstructed);
    }

    #[test]
    fn test_words_not_keywords() {
        let source = "function class enum client";
        let file_id = FileId::new(0);
        let tokens = lex_lossless(source, file_id);

        let kinds: Vec<TokenKind> = tokens
            .iter()
            .filter(|t| t.kind != TokenKind::Whitespace)
            .map(|t| t.kind)
            .collect();

        // All should be Word tokens now
        assert_eq!(
            kinds,
            vec![
                TokenKind::Word,
                TokenKind::Word,
                TokenKind::Word,
                TokenKind::Word,
            ]
        );

        // Verify text is preserved
        let words: Vec<&str> = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Word)
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(words, vec!["function", "class", "enum", "client"]);
    }

    #[test]
    fn test_operators() {
        let source = "-> :: += -= == != <= >= && ||";
        let file_id = FileId::new(0);
        let tokens = lex_lossless(source, file_id);

        let kinds: Vec<TokenKind> = tokens
            .iter()
            .filter(|t| t.kind != TokenKind::Whitespace)
            .map(|t| t.kind)
            .collect();

        assert_eq!(
            kinds,
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
        let file_id = FileId::new(0);
        let tokens = lex_lossless(source, file_id);

        let kinds: Vec<TokenKind> = tokens
            .iter()
            .filter(|t| t.kind != TokenKind::Whitespace)
            .map(|t| t.kind)
            .collect();

        // Should tokenize as: WORD("gpt"), MINUS, INTEGER("4"), WORD("o"), WORD("model"), MINUS, WORD("name")
        // Wait, no - the regex is [a-zA-Z_][a-zA-Z0-9_-]* so hyphens inside words should work
        assert_eq!(kinds, vec![TokenKind::Word, TokenKind::Word,]);

        let words: Vec<&str> = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Word)
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(words, vec!["gpt-4o", "model-name"]);
    }

    #[test]
    fn test_arithmetic_operators() {
        let source = "+ - * / % ++ -- += -= *= /= %=";
        let file_id = FileId::new(0);
        let tokens = lex_lossless(source, file_id);

        let kinds: Vec<TokenKind> = tokens
            .iter()
            .filter(|t| t.kind != TokenKind::Whitespace)
            .map(|t| t.kind)
            .collect();

        assert_eq!(
            kinds,
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
        let source = "& | ^ ~ && || &= |= ^=";
        let file_id = FileId::new(0);
        let tokens = lex_lossless(source, file_id);

        let kinds: Vec<TokenKind> = tokens
            .iter()
            .filter(|t| t.kind != TokenKind::Whitespace)
            .map(|t| t.kind)
            .collect();

        assert_eq!(
            kinds,
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
        let source = "<< >> <<= >>=";
        let file_id = FileId::new(0);
        let tokens = lex_lossless(source, file_id);

        let kinds: Vec<TokenKind> = tokens
            .iter()
            .filter(|t| t.kind != TokenKind::Whitespace)
            .map(|t| t.kind)
            .collect();

        assert_eq!(
            kinds,
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
        let file_id = FileId::new(0);
        let tokens = lex_lossless(source, file_id);

        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::LessLessEquals);

        // Test >> vs >=
        let source2 = ">>= >= >>";
        let tokens2 = lex_lossless(source2, file_id);

        let kinds: Vec<TokenKind> = tokens2
            .iter()
            .filter(|t| t.kind != TokenKind::Whitespace)
            .map(|t| t.kind)
            .collect();

        assert_eq!(
            kinds,
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
        let file_id = FileId::new(0);
        let tokens = lex_lossless(source, file_id);

        // Should lex as: Hash, Quote, Word("Hello"), Word("World"), Quote, Hash
        let kinds: Vec<TokenKind> = tokens
            .iter()
            .filter(|t| t.kind != TokenKind::Whitespace)
            .map(|t| t.kind)
            .collect();

        assert_eq!(
            kinds,
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
        assert_eq!(reconstruct_source(&tokens), source);
    }

    #[test]
    fn test_raw_string_multiple_hashes() {
        // With Quote tokens, quotes inside are just more tokens
        let source = r###"##"String with quotes inside"##"###;
        let file_id = FileId::new(0);
        let tokens = lex_lossless(source, file_id);

        let kinds: Vec<TokenKind> = tokens
            .iter()
            .filter(|t| t.kind != TokenKind::Whitespace)
            .map(|t| t.kind)
            .collect();

        // Hash, Hash, Quote, ...(words)..., Quote, Hash, Hash
        assert_eq!(kinds[0], TokenKind::Hash);
        assert_eq!(kinds[1], TokenKind::Hash);
        assert_eq!(kinds[2], TokenKind::Quote);
        // ... words in between ...
        assert_eq!(kinds[kinds.len() - 3], TokenKind::Quote);
        assert_eq!(kinds[kinds.len() - 2], TokenKind::Hash);
        assert_eq!(kinds[kinds.len() - 1], TokenKind::Hash);

        assert_eq!(reconstruct_source(&tokens), source);
    }

    #[test]
    fn test_raw_string_with_jinja() {
        let source = r##"#"Hello {{ name }}"#"##;
        let file_id = FileId::new(0);
        let tokens = lex_lossless(source, file_id);

        let kinds: Vec<TokenKind> = tokens
            .iter()
            .filter(|t| t.kind != TokenKind::Whitespace)
            .map(|t| t.kind)
            .collect();

        // Should start with: Hash, Quote
        assert_eq!(kinds[0], TokenKind::Hash);
        assert_eq!(kinds[1], TokenKind::Quote);
        // And end with: Quote, Hash
        assert_eq!(kinds[kinds.len() - 2], TokenKind::Quote);
        assert_eq!(kinds[kinds.len() - 1], TokenKind::Hash);

        assert_eq!(reconstruct_source(&tokens), source);
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
        let file_id = FileId::new(0);
        let tokens = lex_lossless(source, file_id);

        let kinds: Vec<TokenKind> = tokens
            .iter()
            .filter(|t| t.kind != TokenKind::Whitespace)
            .map(|t| t.kind)
            .collect();

        // Should start with: Word("prompt"), Hash, Quote
        assert_eq!(kinds[0], TokenKind::Word);
        assert_eq!(kinds[1], TokenKind::Hash);
        assert_eq!(kinds[2], TokenKind::Quote);
        // And end with: Quote, Hash
        assert_eq!(kinds[kinds.len() - 2], TokenKind::Quote);
        assert_eq!(kinds[kinds.len() - 1], TokenKind::Hash);

        // Lossless
        assert_eq!(reconstruct_source(&tokens), source);
    }

    #[test]
    fn test_multiple_raw_strings() {
        let source = r##"#"First"# #"Second"#"##;
        let file_id = FileId::new(0);
        let tokens = lex_lossless(source, file_id);

        let kinds: Vec<TokenKind> = tokens
            .iter()
            .filter(|t| t.kind != TokenKind::Whitespace)
            .map(|t| t.kind)
            .collect();

        // Should be: Hash, Quote, Word, Quote, Hash, Hash, Quote, Word, Quote, Hash
        assert_eq!(kinds[0], TokenKind::Hash);
        assert_eq!(kinds[1], TokenKind::Quote);
        assert_eq!(kinds[2], TokenKind::Word);
        assert_eq!(kinds[3], TokenKind::Quote);
        assert_eq!(kinds[4], TokenKind::Hash);
        assert_eq!(kinds[5], TokenKind::Hash);
        assert_eq!(kinds[6], TokenKind::Quote);
        assert_eq!(kinds[7], TokenKind::Word);
        assert_eq!(kinds[8], TokenKind::Quote);
        assert_eq!(kinds[9], TokenKind::Hash);

        // Lossless
        assert_eq!(reconstruct_source(&tokens), source);
    }

    #[test]
    fn test_five_hash_delimiter() {
        let source = r######"#####"Complex content here"#####"######;
        let file_id = FileId::new(0);
        let tokens = lex_lossless(source, file_id);

        let kinds: Vec<TokenKind> = tokens
            .iter()
            .filter(|t| t.kind != TokenKind::Whitespace)
            .map(|t| t.kind)
            .collect();

        // Should be: 5 Hash, Quote, ...(words)..., Quote, 5 Hash
        for kind in kinds.iter().take(5) {
            assert_eq!(*kind, TokenKind::Hash);
        }
        assert_eq!(kinds[5], TokenKind::Quote);
        // ... words in middle ...
        assert_eq!(kinds[kinds.len() - 6], TokenKind::Quote);
        for kind in kinds.iter().skip(kinds.len() - 5) {
            assert_eq!(*kind, TokenKind::Hash);
        }
        assert_eq!(reconstruct_source(&tokens), source);
    }
}
