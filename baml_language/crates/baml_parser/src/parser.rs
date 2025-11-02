//! Parser implementation.
//!
//! This is a stub that creates minimal valid parse trees.
//! Real grammar implementation will come later.

use baml_lexer::{Token, TokenKind};
use baml_syntax::SyntaxKind;
use rowan::{GreenNode, GreenNodeBuilder};

use crate::ParseError;

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
        // Literals
        TokenKind::Word => SyntaxKind::WORD,
        TokenKind::Quote => SyntaxKind::QUOTE,
        TokenKind::Hash => SyntaxKind::HASH,
        TokenKind::Integer => SyntaxKind::INTEGER,
        TokenKind::Float => SyntaxKind::FLOAT,

        // Brackets
        TokenKind::LBrace => SyntaxKind::L_BRACE,
        TokenKind::RBrace => SyntaxKind::R_BRACE,
        TokenKind::LParen => SyntaxKind::L_PAREN,
        TokenKind::RParen => SyntaxKind::R_PAREN,
        TokenKind::LBracket => SyntaxKind::L_BRACKET,
        TokenKind::RBracket => SyntaxKind::R_BRACKET,

        // Punctuation
        TokenKind::Colon => SyntaxKind::COLON,
        TokenKind::DoubleColon => SyntaxKind::DOUBLE_COLON,
        TokenKind::Comma => SyntaxKind::COMMA,
        TokenKind::Semicolon => SyntaxKind::SEMICOLON,
        TokenKind::Dot => SyntaxKind::DOT,

        // Special operators
        TokenKind::Arrow => SyntaxKind::ARROW,
        TokenKind::At => SyntaxKind::AT,
        TokenKind::AtAt => SyntaxKind::AT_AT,
        TokenKind::Pipe => SyntaxKind::PIPE,
        TokenKind::Question => SyntaxKind::QUESTION,

        // Assignment operators
        TokenKind::Equals => SyntaxKind::EQUALS,
        TokenKind::PlusEquals => SyntaxKind::PLUS_EQUALS,
        TokenKind::MinusEquals => SyntaxKind::MINUS_EQUALS,
        TokenKind::StarEquals => SyntaxKind::STAR_EQUALS,
        TokenKind::SlashEquals => SyntaxKind::SLASH_EQUALS,
        TokenKind::PercentEquals => SyntaxKind::PERCENT_EQUALS,
        TokenKind::AndEquals => SyntaxKind::AND_EQUALS,
        TokenKind::PipeEquals => SyntaxKind::PIPE_EQUALS,
        TokenKind::CaretEquals => SyntaxKind::CARET_EQUALS,
        TokenKind::LessLessEquals => SyntaxKind::LESS_LESS_EQUALS,
        TokenKind::GreaterGreaterEquals => SyntaxKind::GREATER_GREATER_EQUALS,

        // Comparison operators
        TokenKind::EqualsEquals => SyntaxKind::EQUALS_EQUALS,
        TokenKind::NotEquals => SyntaxKind::NOT_EQUALS,
        TokenKind::Less => SyntaxKind::LESS,
        TokenKind::Greater => SyntaxKind::GREATER,
        TokenKind::LessEquals => SyntaxKind::LESS_EQUALS,
        TokenKind::GreaterEquals => SyntaxKind::GREATER_EQUALS,

        // Shift operators
        TokenKind::LessLess => SyntaxKind::LESS_LESS,
        TokenKind::GreaterGreater => SyntaxKind::GREATER_GREATER,

        // Logical operators
        TokenKind::AndAnd => SyntaxKind::AND_AND,
        TokenKind::OrOr => SyntaxKind::OR_OR,
        TokenKind::Not => SyntaxKind::NOT,

        // Bitwise operators
        TokenKind::And => SyntaxKind::AND,
        TokenKind::Caret => SyntaxKind::CARET,
        TokenKind::Tilde => SyntaxKind::TILDE,

        // Arithmetic operators
        TokenKind::Plus => SyntaxKind::PLUS,
        TokenKind::Minus => SyntaxKind::MINUS,
        TokenKind::Star => SyntaxKind::STAR,
        TokenKind::Slash => SyntaxKind::SLASH,
        TokenKind::Percent => SyntaxKind::PERCENT,
        TokenKind::PlusPlus => SyntaxKind::PLUS_PLUS,
        TokenKind::MinusMinus => SyntaxKind::MINUS_MINUS,

        // Whitespace and comments
        TokenKind::Whitespace => SyntaxKind::WHITESPACE,
        TokenKind::Newline => SyntaxKind::NEWLINE,
        TokenKind::LineComment => SyntaxKind::LINE_COMMENT,
        TokenKind::BlockComment => SyntaxKind::BLOCK_COMMENT,

        // Error
        TokenKind::Error => SyntaxKind::ERROR_TOKEN,
    }
}
