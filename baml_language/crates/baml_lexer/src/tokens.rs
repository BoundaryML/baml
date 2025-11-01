//! Token definitions and lexing implementation.

use baml_base::{FileId, Span};
use logos::Logos;
use text_size::{TextRange, TextSize};

/// Token kinds for BAML.
#[derive(Logos, Debug, PartialEq, Eq, Clone, Copy)]
pub enum TokenKind {
    // Keywords
    #[token("function")]
    Function,
    #[token("class")]
    Class,
    #[token("enum")]
    Enum,
    #[token("client")]
    Client,
    #[token("retry_policy")]
    RetryPolicy,
    #[token("test")]
    Test,
    #[token("generator")]
    Generator,

    // Identifiers and literals
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*")]
    Identifier,

    #[regex(r#""([^"\\]|\\.)*""#)]
    String,

    #[regex(r"[0-9]+")]
    Integer,

    #[regex(r"[0-9]+\.[0-9]+")]
    Float,

    // Operators and punctuation
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
    #[token(":")]
    Colon,
    #[token("::")]
    DoubleColon,
    #[token(",")]
    Comma,
    #[token("->")]
    Arrow,
    #[token("@")]
    At,
    #[token("@@")]
    AtAt,
    #[token("|")]
    Pipe,
    #[token("?")]
    Question,
    #[token("=")]
    Equals,

    // Comments (for lossless lexing)
    #[regex(r"//[^\n]*")]
    LineComment,

    #[regex(r"/\*([^*]|\*[^/])*\*/")]
    BlockComment,

    // Whitespace (preserved for lossless lexing)
    #[regex(r"[ \t]+")]
    Whitespace,

    #[regex(r"\r?\n")]
    Newline,

    // Error token for any unrecognized input
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
    use super::*;
    use baml_base::FileId;

    #[test]
    fn test_lossless_lexing() {
        let source = "function test() {}";
        let file_id = FileId::new(0);
        let tokens = lex_lossless(source, file_id);
        let reconstructed = reconstruct_source(&tokens);
        assert_eq!(source, reconstructed);
    }

    #[test]
    fn test_keywords() {
        let source = "function class enum client";
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
                TokenKind::Function,
                TokenKind::Class,
                TokenKind::Enum,
                TokenKind::Client,
            ]
        );
    }
}
