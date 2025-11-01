//! Parser implementation.
//!
//! This is a stub that creates minimal valid parse trees.
//! Real grammar implementation will come later.

use crate::ParseError;
use baml_lexer::{Token, TokenKind};
use baml_syntax::SyntaxKind;
use rowan::{GreenNode, GreenNodeBuilder};

/// Parse tokens into a green tree.
///
/// Returns the green tree and any parse errors encountered.
///
/// **STUB**: Currently just wraps all tokens in a ROOT node.
pub fn parse_file(tokens: &[Token]) -> (GreenNode, Vec<ParseError>) {
    let mut builder = GreenNodeBuilder::new();
    let errors = Vec::new();

    builder.start_node(SyntaxKind::ROOT.into());

    // Stub: Just add all tokens as-is
    for token in tokens {
        let kind = token_kind_to_syntax_kind(token.kind);
        builder.token(kind.into(), &token.text);
    }

    builder.finish_node();

    let green = builder.finish();
    (green, errors)
}

/// Map lexer token kinds to syntax kinds.
fn token_kind_to_syntax_kind(kind: TokenKind) -> SyntaxKind {
    match kind {
        TokenKind::Function => SyntaxKind::FUNCTION_KW,
        TokenKind::Class => SyntaxKind::CLASS_KW,
        TokenKind::Enum => SyntaxKind::ENUM_KW,
        TokenKind::Client => SyntaxKind::CLIENT_KW,
        TokenKind::RetryPolicy => SyntaxKind::RETRY_POLICY_KW,
        TokenKind::Test => SyntaxKind::TEST_KW,
        TokenKind::Generator => SyntaxKind::GENERATOR_KW,

        TokenKind::Identifier => SyntaxKind::IDENTIFIER,
        TokenKind::String => SyntaxKind::STRING,
        TokenKind::Integer => SyntaxKind::INTEGER,
        TokenKind::Float => SyntaxKind::FLOAT,

        TokenKind::LBrace => SyntaxKind::L_BRACE,
        TokenKind::RBrace => SyntaxKind::R_BRACE,
        TokenKind::LParen => SyntaxKind::L_PAREN,
        TokenKind::RParen => SyntaxKind::R_PAREN,
        TokenKind::LBracket => SyntaxKind::L_BRACKET,
        TokenKind::RBracket => SyntaxKind::R_BRACKET,
        TokenKind::Colon => SyntaxKind::COLON,
        TokenKind::DoubleColon => SyntaxKind::DOUBLE_COLON,
        TokenKind::Comma => SyntaxKind::COMMA,
        TokenKind::Arrow => SyntaxKind::ARROW,
        TokenKind::At => SyntaxKind::AT,
        TokenKind::AtAt => SyntaxKind::AT_AT,
        TokenKind::Pipe => SyntaxKind::PIPE,
        TokenKind::Question => SyntaxKind::QUESTION,
        TokenKind::Equals => SyntaxKind::EQUALS,

        TokenKind::Whitespace => SyntaxKind::WHITESPACE,
        TokenKind::Newline => SyntaxKind::NEWLINE,
        TokenKind::LineComment => SyntaxKind::LINE_COMMENT,
        TokenKind::BlockComment => SyntaxKind::BLOCK_COMMENT,

        TokenKind::Error => SyntaxKind::ERROR_TOKEN,
    }
}
