//! Parser implementation.
//!
//! Implements a recursive descent parser with error recovery.

use baml_base::Span;
use baml_compiler_lexer::{Token, TokenKind};
use baml_compiler_syntax::SyntaxKind;
use rowan::{GreenNode, GreenNodeBuilder, NodeCache};
use text_size::TextRange;

use crate::ParseError;

/// Parse tokens using a caller-provided [`NodeCache`] so that identical
/// subtrees from previous parses can be reused.
pub fn parse_file_with_cache(
    tokens: &[Token],
    cache: &mut NodeCache,
) -> (GreenNode, Vec<ParseError>) {
    parse_impl(tokens, Some(cache))
}

pub fn parse_file(tokens: &[Token]) -> (GreenNode, Vec<ParseError>) {
    parse_impl(tokens, None)
}

/// Map lexer token kinds to syntax kinds.
fn token_kind_to_syntax_kind(kind: TokenKind) -> SyntaxKind {
    match kind {
        // Keywords
        TokenKind::Class => SyntaxKind::KW_CLASS,
        TokenKind::Enum => SyntaxKind::KW_ENUM,
        TokenKind::Function => SyntaxKind::KW_FUNCTION,
        TokenKind::Client => SyntaxKind::KW_CLIENT,
        TokenKind::Generator => SyntaxKind::KW_GENERATOR,
        TokenKind::Test => SyntaxKind::KW_TEST,
        TokenKind::RetryPolicy => SyntaxKind::KW_RETRY_POLICY,
        TokenKind::TemplateString => SyntaxKind::KW_TEMPLATE_STRING,
        TokenKind::TypeBuilder => SyntaxKind::KW_TYPE_BUILDER,
        TokenKind::If => SyntaxKind::KW_IF,
        TokenKind::Else => SyntaxKind::KW_ELSE,
        TokenKind::For => SyntaxKind::KW_FOR,
        TokenKind::While => SyntaxKind::KW_WHILE,
        TokenKind::Let => SyntaxKind::KW_LET,
        TokenKind::In => SyntaxKind::KW_IN,
        TokenKind::Break => SyntaxKind::KW_BREAK,
        TokenKind::Continue => SyntaxKind::KW_CONTINUE,
        TokenKind::Return => SyntaxKind::KW_RETURN,
        TokenKind::Watch => SyntaxKind::KW_WATCH,
        TokenKind::Instanceof => SyntaxKind::KW_INSTANCEOF,
        TokenKind::Env => SyntaxKind::KW_ENV,
        TokenKind::Dynamic => SyntaxKind::KW_DYNAMIC,
        TokenKind::Match => SyntaxKind::KW_MATCH,
        TokenKind::Assert => SyntaxKind::KW_ASSERT,

        // Literals
        TokenKind::Word => SyntaxKind::WORD,
        TokenKind::Quote => SyntaxKind::QUOTE,
        TokenKind::Hash => SyntaxKind::HASH,
        TokenKind::IntegerLiteral => SyntaxKind::INTEGER_LITERAL,
        TokenKind::FloatLiteral => SyntaxKind::FLOAT_LITERAL,

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
        TokenKind::DotDotDot => SyntaxKind::DOT_DOT_DOT,
        TokenKind::Dot => SyntaxKind::DOT,
        TokenKind::Dollar => SyntaxKind::DOLLAR,

        // Special operators
        TokenKind::Arrow => SyntaxKind::ARROW,
        TokenKind::FatArrow => SyntaxKind::FAT_ARROW,
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

        // Logical operators
        TokenKind::AndAnd => SyntaxKind::AND_AND,
        TokenKind::OrOr => SyntaxKind::OR_OR,
        TokenKind::Not => SyntaxKind::NOT,

        // Shift operators
        TokenKind::LessLess => SyntaxKind::LESS_LESS,
        TokenKind::GreaterGreater => SyntaxKind::GREATER_GREATER,

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

        // Whitespace
        TokenKind::Whitespace => SyntaxKind::WHITESPACE,
        TokenKind::Newline => SyntaxKind::NEWLINE,

        // Error
        TokenKind::Error => SyntaxKind::ERROR_TOKEN,
    }
}

/// Events for building the syntax tree.
#[derive(Debug, Clone)]
enum Event {
    StartNode {
        kind: SyntaxKind,
    },
    FinishNode,
    Token {
        kind: SyntaxKind,
        text: String,
    },
    UnexpectedToken {
        expected: String,
        found: String,
        span: Span,
    },
    /// A syntax hint with a custom message (not using "Expected/found" format)
    SyntaxHint {
        message: String,
        span: Span,
    },
}

/// Recursive descent parser with error recovery.
pub(crate) struct Parser<'a> {
    tokens: &'a [Token],
    current: usize,
    events: Vec<Event>,
    /// Track pending '>' tokens from split '>>' (for nested generics like `map<K, map<K2, V>>`).
    pending_greaters: u8,
    /// Track the span of the '>>' token that created the pending '>', for error reporting.
    pending_greater_span: Option<Span>,
    /// Track nesting depth of generic type arguments (`TYPE_ARGS`, `GENERIC_ARGS`).
    /// Used to detect unmatched '>' when exiting the outermost generic.
    type_args_depth: u32,
}

impl<'a> Parser<'a> {
    pub(crate) fn new(tokens: &'a [Token]) -> Self {
        Self {
            tokens,
            current: 0,
            events: Vec::new(),
            pending_greaters: 0,
            pending_greater_span: None,
            type_args_depth: 0,
        }
    }

    // ============ Navigation ============

    /// Get current token (skipping all trivia: whitespace, newlines, and comments)
    fn current(&self) -> Option<&Token> {
        self.current_impl(true)
    }

    /// Get current token (skipping only basic trivia: whitespace and newlines, NOT comments)
    /// Use this inside string parsing where // should not be treated as comment start.
    fn current_raw(&self) -> Option<&Token> {
        self.current_impl(false)
    }

    /// Peek ahead n tokens (skipping all trivia: whitespace, newlines, and comments)
    fn peek(&self, n: usize) -> Option<&Token> {
        self.peek_impl(n, true)
    }

    /// Skip a comment pattern starting at position i, returning the new position
    fn skip_comment_at(&self, mut i: usize) -> usize {
        if self.is_line_comment_at(i) {
            // Skip until newline
            i += 2; // Skip //
            while i < self.tokens.len() && self.tokens[i].kind != TokenKind::Newline {
                i += 1;
            }
        } else if self.is_block_comment_at(i) {
            // Skip until */
            i += 2; // Skip /*
            while i < self.tokens.len() {
                if self.tokens[i].kind == TokenKind::Star
                    && i + 1 < self.tokens.len()
                    && self.tokens[i + 1].kind == TokenKind::Slash
                {
                    i += 2; // Skip */
                    break;
                }
                i += 1;
            }
        }
        i
    }

    /// Internal: Get current token, optionally skipping comment patterns
    fn current_impl(&self, skip_comments: bool) -> Option<&Token> {
        let mut i = self.current;
        while i < self.tokens.len() {
            // Skip comment patterns if requested
            if skip_comments {
                let new_i = self.skip_comment_at(i);
                if new_i != i {
                    i = new_i;
                    continue;
                }
            }

            let token = &self.tokens[i];
            if !self.is_basic_trivia(token.kind) {
                return Some(token);
            }
            i += 1;
        }
        None
    }

    /// Internal: Peek ahead n tokens, optionally skipping comment patterns
    fn peek_impl(&self, n: usize, skip_comments: bool) -> Option<&Token> {
        let mut count = 0;
        let mut i = self.current;
        while i < self.tokens.len() {
            // Skip comment patterns if requested
            if skip_comments {
                let new_i = self.skip_comment_at(i);
                if new_i != i {
                    i = new_i;
                    continue;
                }
            }

            let token = &self.tokens[i];
            if !self.is_basic_trivia(token.kind) {
                if count == n {
                    return Some(token);
                }
                count += 1;
            }
            i += 1;
        }
        None
    }

    /// Check if at end of input
    fn at_end(&self) -> bool {
        self.current().is_none()
    }

    /// Check if current token matches the given kind
    fn at(&self, kind: TokenKind) -> bool {
        self.current().map(|t| t.kind == kind).unwrap_or(false)
    }

    /// Check if current token matches the given kind (without skipping comments)
    /// Use this inside string parsing where // should not be treated as comment start.
    fn at_raw(&self, kind: TokenKind) -> bool {
        self.current_raw().map(|t| t.kind == kind).unwrap_or(false)
    }

    /// Check if the current token can start a type expression.
    /// Valid type starts: Word (type name), string literal, integer/float literal, `LParen` (tuple).
    fn is_at_type_start(&self) -> bool {
        self.at(TokenKind::Word)
            || self.at(TokenKind::Quote) // string literal type
            || self.at(TokenKind::Hash) // raw string literal type
            || self.at(TokenKind::IntegerLiteral)
            || self.at(TokenKind::FloatLiteral)
            || self.at(TokenKind::LParen) // tuple/parenthesized type
    }

    /// Check if a token kind is basic trivia (whitespace/newlines, not comments).
    /// Comments are also conceptually trivia, but they're assembled from token patterns (// and /*).
    #[allow(clippy::unused_self)]
    fn is_basic_trivia(&self, kind: TokenKind) -> bool {
        matches!(kind, TokenKind::Whitespace | TokenKind::Newline)
    }

    /// Check if there's a newline before the next non-trivia token.
    /// Comments are treated as trivia for this purpose.
    fn has_newline_ahead(&self) -> bool {
        let mut i = self.current;
        while i < self.tokens.len() {
            // Skip comments (they're trivia for line termination purposes)
            let new_i = self.skip_comment_at(i);
            if new_i != i {
                i = new_i;
                continue;
            }

            let kind = self.tokens[i].kind;
            if kind == TokenKind::Newline {
                return true;
            }
            if !self.is_basic_trivia(kind) {
                return false;
            }
            i += 1;
        }
        false
    }

    /// Check if position i starts a line comment (//) but NOT a header comment (//#)
    fn is_line_comment_at(&self, i: usize) -> bool {
        if i + 1 < self.tokens.len()
            && self.tokens[i].kind == TokenKind::Slash
            && self.tokens[i + 1].kind == TokenKind::Slash
        {
            // Check if it's a header comment (//# ) - those are NOT regular comments
            if i + 2 < self.tokens.len() && self.tokens[i + 2].kind == TokenKind::Hash {
                return false; // It's a header, not a comment to skip
            }
            return true;
        }
        false
    }

    /// Check if position i starts a header comment (//#)
    fn is_header_comment_at(&self, i: usize) -> bool {
        i + 2 < self.tokens.len()
            && self.tokens[i].kind == TokenKind::Slash
            && self.tokens[i + 1].kind == TokenKind::Slash
            && self.tokens[i + 2].kind == TokenKind::Hash
    }

    /// Check if position i starts a block comment (/*)
    fn is_block_comment_at(&self, i: usize) -> bool {
        i + 1 < self.tokens.len()
            && self.tokens[i].kind == TokenKind::Slash
            && self.tokens[i + 1].kind == TokenKind::Star
    }

    /// Check if we're at the start of a line comment (//)
    fn at_line_comment_start(&self) -> bool {
        self.is_line_comment_at(self.current)
    }

    /// Check if we're at the start of a header comment (//#)
    /// This skips trivia (whitespace, newlines, regular comments) to find the actual token position.
    fn at_header_comment_start(&self) -> bool {
        let mut i = self.current;
        // Skip trivia (whitespace, newlines, regular comments) to find the actual token
        while i < self.tokens.len() {
            let kind = self.tokens[i].kind;
            if kind == TokenKind::Whitespace || kind == TokenKind::Newline {
                i += 1;
            } else if self.is_line_comment_at(i) {
                // Skip regular line comment (but not header comments)
                i += 2; // Skip //
                while i < self.tokens.len() && self.tokens[i].kind != TokenKind::Newline {
                    i += 1;
                }
            } else if self.is_block_comment_at(i) {
                // Skip block comment
                i += 2; // Skip /*
                while i < self.tokens.len() {
                    if self.tokens[i].kind == TokenKind::Star
                        && i + 1 < self.tokens.len()
                        && self.tokens[i + 1].kind == TokenKind::Slash
                    {
                        i += 2; // Skip */
                        break;
                    }
                    i += 1;
                }
            } else {
                break;
            }
        }
        self.is_header_comment_at(i)
    }

    /// Get the span of a header comment (from first / to end of line).
    /// Call this before `consume_header_comment` to get the full span.
    fn header_comment_span(&self) -> baml_base::Span {
        let mut i = self.current;
        // Skip trivia to find the start of the header comment
        while i < self.tokens.len() {
            let kind = self.tokens[i].kind;
            if kind == TokenKind::Whitespace || kind == TokenKind::Newline {
                i += 1;
            } else {
                break;
            }
        }

        let start = self
            .tokens
            .get(i)
            .map(|t| t.span.range.start())
            .unwrap_or_default();
        let file_id = self
            .tokens
            .get(i)
            .map(|t| t.span.file_id)
            .unwrap_or(baml_base::FileId::new(0));

        // Find the end (newline or EOF)
        let mut end = start;
        while i < self.tokens.len() {
            let token = &self.tokens[i];
            if token.kind == TokenKind::Newline {
                break;
            }
            end = token.span.range.end();
            i += 1;
        }

        baml_base::Span::new(file_id, TextRange::new(start, end))
    }

    /// Check if we're at the start of a block comment (/*)
    fn at_block_comment_start(&self) -> bool {
        self.is_block_comment_at(self.current)
    }

    /// Consume a line comment (//) as a single `LINE_COMMENT` token
    fn consume_line_comment(&mut self) {
        // Consume both slashes
        let mut text = String::new();
        text.push_str(&self.tokens[self.current].text);
        self.current += 1;
        text.push_str(&self.tokens[self.current].text);
        self.current += 1;

        // Consume everything until newline
        while self.current < self.tokens.len() {
            let token = &self.tokens[self.current];
            if token.kind == TokenKind::Newline {
                break;
            }
            text.push_str(&token.text);
            self.current += 1;
        }

        // Emit as a single token (not wrapped in a node)
        self.events.push(Event::Token {
            kind: SyntaxKind::LINE_COMMENT,
            text,
        });
    }

    /// Consume a block comment (/* ... */) as a single `BLOCK_COMMENT` token
    fn consume_block_comment(&mut self) {
        // Consume /* and everything until */
        let mut text = String::new();
        text.push_str(&self.tokens[self.current].text); // /
        self.current += 1;
        text.push_str(&self.tokens[self.current].text); // *
        self.current += 1;

        // Find the closing */
        let mut found_close = false;
        while self.current < self.tokens.len() {
            let token = &self.tokens[self.current];
            text.push_str(&token.text);
            self.current += 1;

            // Check if we just consumed * and next is /
            if token.kind == TokenKind::Star
                && self.current < self.tokens.len()
                && self.tokens[self.current].kind == TokenKind::Slash
            {
                text.push_str(&self.tokens[self.current].text);
                self.current += 1;
                found_close = true;
                break;
            }
        }

        if !found_close {
            // Unclosed block comment - will be handled as an error by validation
        }

        // Emit as a single token (not wrapped in a node)
        self.events.push(Event::Token {
            kind: SyntaxKind::BLOCK_COMMENT,
            text,
        });
    }

    /// Consume a header comment (//#...) as a `HEADER_COMMENT` node.
    /// Header comments are MDX-style headers: //# Level 1, //## Level 2, etc.
    /// The number of # determines the header level.
    fn consume_header_comment(&mut self) {
        // First, skip any leading trivia (whitespace, newlines, regular comments) and emit them
        while self.current < self.tokens.len() {
            let kind = self.tokens[self.current].kind;
            if kind == TokenKind::Whitespace || kind == TokenKind::Newline {
                self.events.push(Event::Token {
                    kind: token_kind_to_syntax_kind(kind),
                    text: self.tokens[self.current].text.clone(),
                });
                self.current += 1;
            } else if self.is_line_comment_at(self.current) {
                // Consume regular line comment as trivia
                self.consume_line_comment();
            } else if self.is_block_comment_at(self.current) {
                // Consume block comment as trivia
                self.consume_block_comment();
            } else {
                break;
            }
        }

        self.with_node(SyntaxKind::HEADER_COMMENT, |p| {
            // Consume // prefix
            p.events.push(Event::Token {
                kind: SyntaxKind::SLASH,
                text: p.tokens[p.current].text.clone(),
            });
            p.current += 1;
            p.events.push(Event::Token {
                kind: SyntaxKind::SLASH,
                text: p.tokens[p.current].text.clone(),
            });
            p.current += 1;

            // Count and consume # tokens (determines header level)
            while p.current < p.tokens.len() && p.tokens[p.current].kind == TokenKind::Hash {
                p.events.push(Event::Token {
                    kind: SyntaxKind::HASH,
                    text: p.tokens[p.current].text.clone(),
                });
                p.current += 1;
            }

            // Consume the rest of the line (header title content)
            while p.current < p.tokens.len() {
                let token = &p.tokens[p.current];
                if token.kind == TokenKind::Newline {
                    break;
                }
                // Emit each token with its original kind
                p.events.push(Event::Token {
                    kind: token_kind_to_syntax_kind(token.kind),
                    text: token.text.clone(),
                });
                p.current += 1;
            }
        });
    }

    // ============ Error Recovery Helpers ============`

    /// Check if the current token is a top-level keyword.
    /// Used for error recovery to break out of malformed blocks.
    fn at_top_level_keyword(&self) -> bool {
        matches!(
            self.current().map(|t| t.kind),
            Some(
                TokenKind::Class
                    | TokenKind::Enum
                    | TokenKind::Function
                    | TokenKind::Client
                    | TokenKind::Generator
                    | TokenKind::Test
                    | TokenKind::RetryPolicy
                    | TokenKind::TemplateString
                    | TokenKind::TypeBuilder
            )
        )
    }

    /// Expect a '>' token, but also accept '>>' and consume only one '>'.
    /// This handles nested generics like `map<K, map<K2, V>>` where the lexer
    /// tokenizes '>>' as a single token.
    ///
    /// Returns true if a '>' was consumed (either standalone or as part of '>>').
    fn expect_greater(&mut self) -> bool {
        // First check if we have a pending '>' from a previous '>>' split.
        // Don't emit anything - the '>>' token is already in the tree.
        if self.pending_greaters > 0 {
            self.pending_greaters -= 1;
            if self.pending_greaters == 0 {
                self.pending_greater_span = None;
            }
            return true;
        }

        if self.at(TokenKind::Greater) {
            self.bump();
            true
        } else if self.at(TokenKind::GreaterGreater) {
            // Handle '>>' as two '>':
            // - Consume the '>>' token (adds it to tree once)
            // - Track that the second '>' is pending for the outer generic
            let span = self.current().map(|t| t.span);
            self.bump();
            self.pending_greaters += 1;
            self.pending_greater_span = span;
            true
        } else {
            self.error_unexpected_token("'>'".to_string());
            false
        }
    }

    /// Skip tokens until we find a balanced closing parenthesis.
    /// Used for error recovery in tuple/parenthesized type expressions.
    fn skip_to_balanced_paren(&mut self) {
        let mut paren_depth = 1;
        let mut bracket_depth = 0;
        while !self.at_end() && paren_depth > 0 {
            match self.current().map(|t| t.kind) {
                Some(TokenKind::LParen) => {
                    paren_depth += 1;
                    self.bump();
                }
                Some(TokenKind::RParen) => {
                    paren_depth -= 1;
                    if paren_depth > 0 {
                        self.bump();
                    }
                    // Don't bump the final ')' - let the caller consume it
                }
                Some(TokenKind::LBracket) => {
                    bracket_depth += 1;
                    self.bump();
                }
                Some(TokenKind::RBracket) => {
                    if bracket_depth > 0 {
                        bracket_depth -= 1;
                        self.bump();
                    } else {
                        // Unbalanced ] - stop here
                        break;
                    }
                }
                Some(TokenKind::RBrace) => {
                    // Hit a closing brace - likely at a higher level, stop here
                    break;
                }
                _ => self.bump(),
            }
        }
    }

    /// Try to recover from an invalid top-level block like `classs Foo { ... }`.
    ///
    /// Recognizes the pattern: identifier identifier { ... } (where the first identifier
    /// looks like a typo for a keyword like class/enum/function).
    ///
    /// Returns true if recovery was performed, false otherwise.
    fn try_recover_invalid_block(&mut self) -> bool {
        // Check pattern: Word Word LBrace
        let is_word = self.at(TokenKind::Word);
        let next_is_word = self
            .peek(1)
            .map(|t| t.kind == TokenKind::Word)
            .unwrap_or(false);
        let then_lbrace = self
            .peek(2)
            .map(|t| t.kind == TokenKind::LBrace)
            .unwrap_or(false);

        if !is_word || !next_is_word || !then_lbrace {
            return false;
        }

        // Get the invalid keyword text for the error message
        let invalid_keyword = self.current().map(|t| t.text.clone()).unwrap_or_default();
        let span = self.current().map(|t| t.span).unwrap_or_default();

        // Emit a helpful error message
        self.error(
            format!(
                "Unknown keyword '{invalid_keyword}'. Expected 'class', 'enum', 'function', 'client', 'generator', 'test', or 'type'."
            ),
            span,
        );

        // Wrap the invalid block in an ERROR node
        self.start_node(SyntaxKind::ERROR);

        // Skip the invalid keyword and name
        self.bump(); // invalid keyword (e.g., "classs")
        self.bump(); // name (e.g., "WrongClass")

        // Skip to matching closing brace
        if self.at(TokenKind::LBrace) {
            self.bump(); // consume '{'
            let mut brace_depth = 1;

            while !self.at_end() && brace_depth > 0 {
                match self.current().map(|t| t.kind) {
                    Some(TokenKind::LBrace) => {
                        brace_depth += 1;
                        self.bump();
                    }
                    Some(TokenKind::RBrace) => {
                        brace_depth -= 1;
                        self.bump();
                    }
                    _ => self.bump(),
                }
            }
        }

        self.finish_node();
        true
    }

    /// Try to recover from an invalid type alias declaration like "typpe Name = expr".
    /// Returns true if recovery was performed.
    fn try_recover_invalid_type_alias(&mut self) -> bool {
        // Check pattern: Word Word Equals
        let is_word = self.at(TokenKind::Word);
        let next_is_word = self
            .peek(1)
            .map(|t| t.kind == TokenKind::Word)
            .unwrap_or(false);
        let then_equals = self
            .peek(2)
            .map(|t| t.kind == TokenKind::Equals)
            .unwrap_or(false);

        if !is_word || !next_is_word || !then_equals {
            return false;
        }

        // Get the invalid keyword text for the error message
        let invalid_keyword = self.current().map(|t| t.text.clone()).unwrap_or_default();
        let span = self.current().map(|t| t.span).unwrap_or_default();

        // Emit a helpful error message
        self.error(
            format!(
                "Unknown keyword '{invalid_keyword}'. Did you mean 'type'? Usage: type Name = expression"
            ),
            span,
        );

        // Wrap the invalid type alias in an ERROR node
        self.start_node(SyntaxKind::ERROR);

        // Skip the invalid keyword, name, and = sign
        self.bump(); // invalid keyword (e.g., "typpe")
        self.bump(); // name (e.g., "Two")
        self.bump(); // =

        // Skip to end of line (type alias expressions are typically one line)
        while !self.at_end()
            && !self.at(TokenKind::Newline)
            && !self.at(TokenKind::LBrace)
            && !self.at(TokenKind::RBrace)
        {
            // Stop at keywords that would start a new declaration
            if matches!(
                self.current().map(|t| t.kind),
                Some(
                    TokenKind::Class
                        | TokenKind::Enum
                        | TokenKind::Function
                        | TokenKind::Client
                        | TokenKind::Generator
                        | TokenKind::Test
                )
            ) {
                break;
            }
            self.bump();
        }

        self.finish_node();
        true
    }

    // ============ Consumption ============

    /// Consume current token, including all trivia before it (whitespace, newlines, comments).
    /// This is used for normal top-level parsing.
    fn bump(&mut self) {
        self.bump_impl(true);
    }

    /// Consume current token, including only basic trivia (whitespace, newlines).
    /// Does NOT recognize comment patterns - treats // and /* as literal tokens.
    /// This is used when parsing string content where // should not start a comment.
    fn bump_raw(&mut self) {
        self.bump_impl(false);
    }

    /// Internal: Consume current token with optional comment pattern recognition
    fn bump_impl(&mut self, recognize_comments: bool) {
        // Emit all trivia before the token
        while self.current < self.tokens.len() {
            // Recognize and assemble comment patterns if requested
            if recognize_comments {
                if self.at_line_comment_start() {
                    self.consume_line_comment();
                    continue;
                }
                if self.at_block_comment_start() {
                    self.consume_block_comment();
                    continue;
                }
            }

            let token = &self.tokens[self.current];

            // Emit basic trivia (whitespace, newlines)
            if self.is_basic_trivia(token.kind) {
                let kind = token_kind_to_syntax_kind(token.kind);
                self.events.push(Event::Token {
                    kind,
                    text: token.text.clone(),
                });
                self.current += 1;
                continue;
            }

            // Non-trivia token - emit it and stop
            let kind = token_kind_to_syntax_kind(token.kind);
            self.events.push(Event::Token {
                kind,
                text: token.text.clone(),
            });
            self.current += 1;
            break;
        }
    }

    /// Consume token if it matches expected kind
    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Expect a token, emit error if not found
    fn expect(&mut self, kind: TokenKind) -> bool {
        if self.eat(kind) {
            true
        } else {
            let found = self
                .current()
                .map(|t| format!("{}", t.kind))
                .unwrap_or_else(|| "EOF".to_string());

            let span = self.current().map(|t| t.span).unwrap_or_else(|| {
                // Use the span of the last token if available, or a default empty span
                self.tokens.last().map(|t| t.span).unwrap_or_else(|| {
                    baml_base::Span::new(baml_base::FileId::new(0), TextRange::default())
                })
            });

            self.events.push(Event::UnexpectedToken {
                expected: format!("{kind}"),
                found,
                span,
            });
            false
        }
    }

    // ============ Tree Building ============

    fn start_node(&mut self, kind: SyntaxKind) {
        self.events.push(Event::StartNode { kind });
    }

    fn finish_node(&mut self) {
        self.events.push(Event::FinishNode);
    }

    fn error_unexpected_token(&mut self, expected: String) {
        let found = self
            .current()
            .map(|t| format!("{}", t.kind))
            .unwrap_or_else(|| "EOF".to_string());

        let span = self.current().map(|t| t.span).unwrap_or_else(|| {
            // Use the span of the last token if available, or a default empty span
            self.tokens.last().map(|t| t.span).unwrap_or_else(|| {
                baml_base::Span::new(baml_base::FileId::new(0), TextRange::default())
            })
        });

        self.events.push(Event::UnexpectedToken {
            expected,
            found,
            span,
        });
    }

    /// Emit a syntax hint with a custom message and span
    fn error(&mut self, message: String, span: baml_base::Span) {
        self.events.push(Event::SyntaxHint { message, span });
    }

    /// Parse with a node wrapper
    fn with_node<F>(&mut self, kind: SyntaxKind, f: F)
    where
        F: FnOnce(&mut Self),
    {
        self.start_node(kind);
        f(self);
        self.finish_node();
    }

    // ============ Building the Tree ============

    fn build_tree(self, cache: Option<&mut NodeCache>) -> (GreenNode, Vec<ParseError>) {
        // eprintln!("[BUILD_TREE] Starting with {} events", self.events.len());
        let mut builder = if let Some(cache) = cache {
            GreenNodeBuilder::with_cache(cache)
        } else {
            GreenNodeBuilder::new()
        };
        let mut errors = Vec::new();

        for event in self.events {
            match event {
                Event::StartNode { kind } => {
                    builder.start_node(kind.into());
                }
                Event::FinishNode => {
                    builder.finish_node();
                }
                Event::Token { kind, text } => {
                    builder.token(kind.into(), &text);
                }
                Event::UnexpectedToken {
                    expected,
                    found,
                    span,
                } => {
                    errors.push(ParseError::UnexpectedToken {
                        expected,
                        found,
                        span,
                    });
                }
                Event::SyntaxHint { message, span } => {
                    errors.push(ParseError::InvalidSyntax { message, span });
                }
            }
        }

        (builder.finish(), errors)
    }

    // ============ String Parsing ============

    /// Count consecutive Hash tokens starting at current position (skipping basic trivia only)
    fn count_consecutive_hashes(&self) -> usize {
        let mut count = 0;
        let mut i = self.current;

        while i < self.tokens.len() {
            let token = &self.tokens[i];
            if token.kind == TokenKind::Hash {
                count += 1;
                i += 1;
            } else if self.is_basic_trivia(token.kind) {
                i += 1;
            } else {
                break;
            }
        }

        count
    }

    /// Find the token position after consuming N hashes (skipping basic trivia only)
    fn find_token_after_hashes(&self, hash_count: usize) -> Option<usize> {
        let mut hashes_seen = 0;
        let mut i = self.current;

        while i < self.tokens.len() {
            let token = &self.tokens[i];
            if token.kind == TokenKind::Hash {
                hashes_seen += 1;
                i += 1;
                if hashes_seen == hash_count {
                    // Found all hashes, now skip basic trivia to find next token
                    while i < self.tokens.len() && self.is_basic_trivia(self.tokens[i].kind) {
                        i += 1;
                    }
                    return Some(i);
                }
            } else if self.is_basic_trivia(token.kind) {
                i += 1;
            } else {
                break;
            }
        }

        None
    }

    /// Count Hash tokens immediately after current Quote token (skipping basic trivia only)
    fn count_consecutive_hashes_after_quote(&self) -> usize {
        let mut count = 0;
        // First, find the actual position of the current token (skipping trivia from self.current)
        let mut i = self.current;
        while i < self.tokens.len() && self.is_basic_trivia(self.tokens[i].kind) {
            i += 1;
        }
        // Now i is at the Quote token, move past it
        i += 1;

        // Count consecutive hashes after the quote
        while i < self.tokens.len() {
            let token = &self.tokens[i];
            if token.kind == TokenKind::Hash {
                count += 1;
                i += 1;
            } else if self.is_basic_trivia(token.kind) {
                i += 1;
            } else {
                break;
            }
        }

        count
    }

    /// Parse a string literal
    /// Lexer emits: Quote, (content tokens), Quote
    /// Parser assembles: `STRING_LITERAL` node
    pub(crate) fn parse_string(&mut self) -> bool {
        // eprintln!("[PARSE_STRING] Starting at pos {}", self.current);
        if !self.at(TokenKind::Quote) {
            return false;
        }

        self.with_node(SyntaxKind::STRING_LITERAL, |p| {
            p.bump(); // Opening quote

            // Collect all tokens until closing quote
            let mut loop_counter = 0;
            while !p.at_end() {
                loop_counter += 1;
                if loop_counter > 100_000 {
                    p.error_unexpected_token("String parsing exceeded iteration limit".to_string());
                    return;
                }

                // Check if next token is the closing quote
                // Use at_raw to avoid skipping // as comments - we want the actual next token
                if p.at_raw(TokenKind::Quote) {
                    p.bump_raw(); // Consume closing quote
                    return;
                }
                // Not a quote - consume as string content
                // Use bump_raw() to avoid treating // as comments inside strings
                p.bump_raw();
            }

            // If we get here, we reached EOF without finding closing quote
            // eprintln!("[PARSE_STRING] Reached EOF without closing quote");
            p.error_unexpected_token("Unclosed string literal".to_string());
        });

        true
    }

    /// Parse a raw string literal with hash delimiters
    /// Lexer emits: Hash+, Quote, (content tokens), Quote, Hash+
    /// Parser assembles and validates matching hash counts
    pub(crate) fn parse_raw_string(&mut self) -> bool {
        if !self.at(TokenKind::Hash) {
            return false;
        }

        // Count opening hashes
        let opening_hashes = self.count_consecutive_hashes();
        if opening_hashes == 0 {
            return false;
        }

        // Must be followed by opening quote - check after consuming hashes
        // We need to peek ahead past the hashes to see if there's a quote
        let quote_pos = self.find_token_after_hashes(opening_hashes);
        if quote_pos.is_none() || quote_pos.map(|i| self.tokens[i].kind) != Some(TokenKind::Quote) {
            // Just hashes, not a raw string
            return false;
        }

        self.with_node(SyntaxKind::RAW_STRING_LITERAL, |p| {
            // Consume opening hashes
            for _ in 0..opening_hashes {
                p.bump(); // #
            }
            p.bump(); // Opening "

            // Collect content until we find Quote followed by same number of hashes
            let mut loop_counter = 0;
            loop {
                loop_counter += 1;
                if loop_counter > 100_000 {
                    p.error_unexpected_token(
                        "Raw string parsing exceeded iteration limit".to_string(),
                    );
                    break;
                }

                if p.at_end() {
                    p.error_unexpected_token(format!(
                        "Unclosed raw string (expected \"{}\")",
                        "#".repeat(opening_hashes)
                    ));
                    break;
                }

                if p.at(TokenKind::Quote) {
                    // Check if followed by correct number of hashes
                    let closing_hashes = p.count_consecutive_hashes_after_quote();
                    if closing_hashes == opening_hashes {
                        // Found matching closing delimiter
                        p.bump(); // Closing "
                        for _ in 0..closing_hashes {
                            p.bump(); // #
                        }
                        break;
                    }
                }

                // Not the closing delimiter, consume as content
                // Use bump_raw() to avoid treating // as comments inside raw strings
                p.bump_raw();
            }
        });

        true
    }

    /// Parse a string or raw string (dispatches to correct method)
    pub(crate) fn parse_any_string(&mut self) -> bool {
        if self.at(TokenKind::Hash) {
            self.parse_raw_string()
        } else if self.at(TokenKind::Quote) {
            self.parse_string()
        } else {
            false
        }
    }

    // ============ Attribute Parsing ============

    /// Parse a field attribute: @alias("name") or @stream.done
    pub(crate) fn parse_field_attribute(&mut self) {
        self.with_node(SyntaxKind::ATTRIBUTE, |p| {
            p.expect(TokenKind::At);

            // Attribute name (can be dotted like stream.done)
            // Allow keywords like 'assert' as attribute names (for @assert)
            if p.at(TokenKind::Word) || p.at(TokenKind::Assert) {
                p.bump();
                // Handle dotted attribute names like @stream.done
                while p.at(TokenKind::Dot) {
                    p.bump(); // consume dot
                    if p.at(TokenKind::Word) || p.at(TokenKind::Assert) {
                        p.bump(); // consume next segment
                    } else {
                        p.error_unexpected_token("attribute name segment after dot".to_string());
                        break;
                    }
                }
            } else {
                p.error_unexpected_token("attribute name".to_string());
                return;
            }

            // Optional arguments in parentheses
            if p.at(TokenKind::LParen) {
                p.parse_attribute_args();
            }
        });
    }

    /// Parse a block attribute: @@dynamic or @@stream.done
    pub(crate) fn parse_block_attribute(&mut self) {
        self.with_node(SyntaxKind::BLOCK_ATTRIBUTE, |p| {
            p.expect(TokenKind::AtAt);

            // Attribute name (can be dotted like @@stream.done)
            if p.at(TokenKind::Word) || p.at(TokenKind::Dynamic) || p.at(TokenKind::Assert) {
                p.bump();
                // Handle dotted attribute names like @@stream.done
                while p.at(TokenKind::Dot) {
                    p.bump(); // consume dot
                    if p.at(TokenKind::Word) || p.at(TokenKind::Dynamic) || p.at(TokenKind::Assert)
                    {
                        p.bump(); // consume next segment
                    } else {
                        p.error_unexpected_token("attribute name segment after dot".to_string());
                        break;
                    }
                }
            } else {
                p.error_unexpected_token("attribute name".to_string());
                return;
            }

            // Optional arguments in parentheses
            if p.at(TokenKind::LParen) {
                p.parse_attribute_args();
            }
        });
    }

    fn parse_attribute_args(&mut self) {
        self.with_node(SyntaxKind::ATTRIBUTE_ARGS, |p| {
            p.expect(TokenKind::LParen);

            // Parse first argument
            if !p.at(TokenKind::RParen) {
                p.parse_attribute_arg();

                // Parse remaining arguments
                while p.eat(TokenKind::Comma) {
                    if p.at(TokenKind::RParen) {
                        break; // Trailing comma
                    }
                    p.parse_attribute_arg();
                }
            }

            p.expect(TokenKind::RParen);
        });
    }

    fn parse_attribute_arg(&mut self) {
        // Attribute argument can be:
        // - String: @alias("user_name")
        // - Raw string: @description(#"Multi-line\ndescription"#)
        // - Expression: @assert({{ this > 0 }})
        // - Unquoted string: @description(User is happy) - consumes until ) or ,

        if self.parse_any_string() {
            // String argument parsed
        } else if self.at(TokenKind::LBrace)
            && self
                .peek(1)
                .map(|t| t.kind == TokenKind::LBrace)
                .unwrap_or(false)
        {
            // Expression block: {{ }}
            self.parse_expression_block();
        } else if self.at(TokenKind::Word) {
            // Unquoted string: consume all tokens until ) or ,
            self.with_node(SyntaxKind::UNQUOTED_STRING, |p| {
                while !p.at(TokenKind::RParen) && !p.at(TokenKind::Comma) && !p.at_end() {
                    p.bump();
                }
            });
        } else {
            self.error_unexpected_token("attribute argument".to_string());
        }
    }

    /// Placeholder for expression block parsing (Phase 4)
    fn parse_expression_block(&mut self) {
        // For now, just consume the {{ }} tokens
        self.with_node(SyntaxKind::EXPR, |p| {
            p.bump(); // {
            p.bump(); // {

            // Consume until }}
            while !p.at_end() {
                if p.at(TokenKind::RBrace)
                    && p.peek(1)
                        .map(|t| t.kind == TokenKind::RBrace)
                        .unwrap_or(false)
                {
                    p.bump(); // }
                    p.bump(); // }
                    break;
                }
                p.bump();
            }
        });
    }

    // ============ Type Parsing ============

    /// Parse a type expression
    /// Examples: string, int, User, string[], map<string, int>, string | int
    /// Can also use string literals: "user", "assistant"
    pub(crate) fn parse_type(&mut self) {
        self.with_node(SyntaxKind::TYPE_EXPR, |p| {
            p.parse_type_primary();

            // Type modifiers
            loop {
                if p.at(TokenKind::LBracket) {
                    // Array type: string[]
                    p.bump(); // [
                    p.expect(TokenKind::RBracket); // ]
                } else if p.at(TokenKind::Question) {
                    // Optional type: string?
                    p.bump();
                } else if p.at(TokenKind::Pipe) {
                    // Union type: string | int | "user" | "assistant"
                    p.bump();
                    p.parse_type_primary();
                } else {
                    break;
                }
            }
        });
    }

    fn parse_type_primary(&mut self) {
        // Check for string literal types: "user" | "assistant"
        if self.parse_any_string() {
            return;
        }

        // Check for integer literal types: 200 | 201 | 204
        // Used for exhaustiveness checking on literal unions
        if self.at(TokenKind::IntegerLiteral) {
            self.bump();
            return;
        }

        // Float literal types are not supported - emit error at parse time
        if self.at(TokenKind::FloatLiteral) {
            if let Some(token) = self.current() {
                self.error(
                    format!("Float literal values are not supported: {}", token.text),
                    token.span,
                );
            }
            self.bump(); // consume to recover
            return;
        }

        if self.at(TokenKind::Word) {
            // Base type name, generic type, or boolean literal (true/false)
            // Note: true/false are Word tokens, and they become BoolLiteral types
            self.bump();

            // Check for generic arguments: map<K, V>
            if self.at(TokenKind::Less) {
                self.type_args_depth += 1;
                self.with_node(SyntaxKind::TYPE_ARGS, |p| {
                    p.bump(); // <

                    p.parse_type();

                    while p.eat(TokenKind::Comma) {
                        p.parse_type();
                    }

                    p.expect_greater();
                });
                self.type_args_depth -= 1;

                // If we just exited the outermost generic and have pending '>', report error
                if self.type_args_depth == 0 && self.pending_greaters > 0 {
                    if let Some(span) = self.pending_greater_span {
                        self.error(
                            format!(
                                "Unmatched '>' in type expression (found {} extra)",
                                self.pending_greaters
                            ),
                            span,
                        );
                    }
                    self.pending_greaters = 0;
                    self.pending_greater_span = None;
                }
            }
        } else if self.at(TokenKind::LParen) {
            // Tuple type or parenthesized type
            self.bump(); // (
            self.parse_type();
            while self.eat(TokenKind::Comma) {
                self.parse_type();
            }
            // Error recovery: if we're not at ')' yet, skip tokens until we find ')' or reach a recovery point
            if !self.at(TokenKind::RParen) {
                if let Some(token) = self.current() {
                    let message = if token.kind == TokenKind::Dot {
                        "Path identifiers (e.g., 'a.b') are not supported in type expressions"
                            .to_string()
                    } else {
                        format!("Unexpected '{}' in type expression", token.text)
                    };
                    self.error(message, token.span);
                }
                self.skip_to_balanced_paren();
            }
            self.expect(TokenKind::RParen);
        } else {
            self.error_unexpected_token("type".to_string());
        }
    }

    // ============ Enum Parsing ============

    /// Parse an enum declaration
    pub(crate) fn parse_enum(&mut self) {
        self.with_node(SyntaxKind::ENUM_DEF, |p| {
            // 'enum' keyword
            p.expect(TokenKind::Enum);

            // Enum name
            if p.at(TokenKind::Word) {
                p.bump(); // name
            } else {
                p.error_unexpected_token("enum name".to_string());
            }

            // Opening brace
            if !p.expect(TokenKind::LBrace) {
                return; // Error recovery: stop here
            }

            // Parse enum variants and attributes
            while !p.at(TokenKind::RBrace) && !p.at_end() {
                // Error recovery: if we see a top-level keyword, assume we missed a closing brace
                if p.at_top_level_keyword() {
                    break;
                }

                if p.at(TokenKind::AtAt) {
                    // Block attribute: @@dynamic
                    p.parse_block_attribute();
                } else if p.at(TokenKind::Word) {
                    // Enum variant
                    p.parse_enum_variant();
                    // Optional comma after variant (allows both comma and no-comma styles)
                    p.eat(TokenKind::Comma);
                } else {
                    // Skip unexpected token
                    p.error_unexpected_token("Unexpected token in enum body".to_string());
                    p.bump();
                }
            }

            // Closing brace
            p.expect(TokenKind::RBrace);
        });
    }

    fn parse_enum_variant(&mut self) {
        self.with_node(SyntaxKind::ENUM_VARIANT, |p| {
            // Variant name
            p.bump();

            // Optional field attributes (@alias, etc.)
            while p.at(TokenKind::At) && !p.at(TokenKind::AtAt) {
                p.parse_field_attribute();
            }
        });
    }

    // ============ Class Parsing ============

    /// Parse a class declaration
    pub(crate) fn parse_class(&mut self) {
        self.with_node(SyntaxKind::CLASS_DEF, |p| {
            // 'class' keyword
            p.expect(TokenKind::Class);

            // Class name
            if p.at(TokenKind::Word) {
                p.bump(); // name
            } else {
                p.error_unexpected_token("class name".to_string());
            }

            // Opening brace
            if !p.expect(TokenKind::LBrace) {
                return;
            }

            // Parse fields, methods, and attributes
            while !p.at(TokenKind::RBrace) && !p.at_end() {
                // Error recovery: if we see a top-level keyword (except function), assume we missed a closing brace
                if p.at_top_level_keyword() && !p.at(TokenKind::Function) {
                    break;
                }

                if p.at(TokenKind::AtAt) {
                    // Block attribute: @@dynamic
                    p.parse_block_attribute();
                } else if p.at(TokenKind::Function) {
                    // Method definition
                    p.parse_function();
                } else if p.at(TokenKind::Word) {
                    // Field declaration
                    p.parse_field();
                } else {
                    // Skip unexpected token
                    p.error_unexpected_token("Unexpected token in class body".to_string());
                    p.bump();
                }
            }

            // Closing brace
            p.expect(TokenKind::RBrace);
        });
    }

    fn parse_field(&mut self) {
        self.with_node(SyntaxKind::FIELD, |p| {
            // Field name - capture span and text before bumping
            let field_name_span = p.current().map(|t| t.span);
            let field_name_text = p.current().map(|t| t.text.clone());
            p.bump();

            // Check if there's a newline before the next token
            // (newline means the type is on a different line - the field is incomplete)
            let newline_before_type = p.has_newline_ahead();

            // Field type - check if we're at a valid type start AND no newline separates them
            let has_type = p.is_at_type_start() && !newline_before_type;
            if has_type {
                p.parse_type();

                // Optional field attributes (@alias, @description, @assert, etc.)
                while p.at(TokenKind::At) && !p.at(TokenKind::AtAt) {
                    p.parse_field_attribute();
                }
            } else {
                // Field is incomplete - emit error and don't consume more tokens
                if let Some(span) = field_name_span {
                    let name = field_name_text.as_deref().unwrap_or("field");
                    p.error(format!("field '{name}' is missing a type annotation"), span);
                }
            }
        });
    }

    // ============ Function Parsing ============

    /// Parse a function declaration with speculative parsing for body type
    pub(crate) fn parse_function(&mut self) {
        self.with_node(SyntaxKind::FUNCTION_DEF, |p| {
            // 'function' keyword
            p.expect(TokenKind::Function);

            // Function name
            if p.at(TokenKind::Word) {
                p.bump();
            } else {
                p.error_unexpected_token("function name".to_string());
                // Recovery: skip until we see '(', '{', or '->'
                while !p.at(TokenKind::LParen)
                    && !p.at(TokenKind::LBrace)
                    && !p.at(TokenKind::Arrow)
                    && !p.at_end()
                {
                    p.bump();
                }
            }

            // Check for old-style function syntax: `function Name {` (without parens and return type)
            // If we see '{' directly after the name, emit a single helpful error and skip to body
            if p.at(TokenKind::LBrace) {
                let span = p.current().map(|t| t.span).unwrap_or_default();
                p.error(
                    "Old-style function syntax. Use: function Name(params...) -> ReturnType { ... }".to_string(),
                    span,
                );
                // Create empty parameter list node for AST consistency
                p.start_node(SyntaxKind::PARAMETER_LIST);
                p.finish_node();
                // Parse the body
                p.parse_function_body();
                return;
            }

            // Parameters
            p.parse_parameter_list();

            // Return type
            if p.eat(TokenKind::Arrow) {
                p.parse_type();
                // Optional attributes on return type (e.g., @check)
                while p.at(TokenKind::At) && !p.at(TokenKind::AtAt) {
                    p.parse_field_attribute();
                }
            } else {
                p.error_unexpected_token("return type (->)".to_string());
            }

            // Body
            if p.at(TokenKind::LBrace) {
                p.parse_function_body();
            } else {
                p.error_unexpected_token("function body".to_string());
            }
        });
    }

    fn parse_parameter_list(&mut self) {
        self.with_node(SyntaxKind::PARAMETER_LIST, |p| {
            p.expect(TokenKind::LParen);

            if !p.at(TokenKind::RParen) {
                p.parse_parameter();

                while p.eat(TokenKind::Comma) {
                    if p.at(TokenKind::RParen) {
                        break; // Trailing comma
                    }
                    p.parse_parameter();
                }
            }

            p.expect(TokenKind::RParen);
        });
    }

    fn parse_parameter(&mut self) {
        self.with_node(SyntaxKind::PARAMETER, |p| {
            // Check if this is a 'self' parameter (no type annotation allowed)
            let is_self = p.current().map(|t| t.text == "self").unwrap_or(false);

            // Parameter name
            if p.at(TokenKind::Word) {
                p.bump();
            } else {
                p.error_unexpected_token("parameter name".to_string());
            }

            // Type annotation - supports both "name: type" and "name type" syntax
            // 'self' parameter does not have a type annotation
            if is_self {
                // No type annotation for self
            } else if p.eat(TokenKind::Colon) {
                // With colon: "name: type"
                p.parse_type();
            } else if p.at(TokenKind::Word) {
                // Without colon: "name type" (whitespace-separated)
                p.parse_type();
            } else {
                p.error_unexpected_token("type annotation".to_string());
            }

            // Optional attributes on parameter (e.g., @assert, @check)
            while p.at(TokenKind::At) && !p.at(TokenKind::AtAt) {
                p.parse_field_attribute();
            }
        });
    }

    fn parse_function_body(&mut self) {
        // Scan tokens to determine function type before parsing (single pass)
        if self.looks_like_llm_function_body() {
            self.parse_llm_function_body();
        } else {
            self.parse_expr_function_body();
        }
    }

    /// Scan tokens to detect if this looks like an LLM function body.
    /// LLM functions contain `client` and `prompt` keywords at brace depth 1.
    /// Expression functions contain `let`, `return`, `if`, `while`, `for`.
    fn looks_like_llm_function_body(&self) -> bool {
        let mut i = self.current;
        let mut brace_depth = 0;

        while i < self.tokens.len() {
            let token = &self.tokens[i];
            match token.kind {
                TokenKind::LBrace => brace_depth += 1,
                TokenKind::RBrace if brace_depth == 1 => break,
                TokenKind::RBrace => brace_depth -= 1,
                TokenKind::Word if brace_depth == 1 => {
                    let text = &token.text;
                    if text == "client" || text == "prompt" {
                        return true;
                    }
                    if text == "let"
                        || text == "return"
                        || text == "if"
                        || text == "while"
                        || text == "for"
                    {
                        return false;
                    }
                }
                // Check for Client keyword token (not just Word with text "client")
                TokenKind::Client if brace_depth == 1 => return true,
                _ => {}
            }
            i += 1;
        }
        false // default to expression function
    }

    fn parse_llm_function_body(&mut self) {
        self.with_node(SyntaxKind::LLM_FUNCTION_BODY, |p| {
            p.expect(TokenKind::LBrace);

            let mut has_client = false;
            let mut has_prompt = false;

            while !p.at(TokenKind::RBrace) && !p.at_end() {
                // Error recovery: if we see a top-level keyword (except Client, which is valid in LLM bodies)
                // assume we missed a closing brace
                if p.at_top_level_keyword() && !p.at(TokenKind::Client) {
                    break;
                }

                // Check for header comments - not allowed in LLM functions
                if p.at_header_comment_start() {
                    let span = p.header_comment_span();
                    p.error(
                        "Header comments (//#) are not allowed inside LLM functions".to_string(),
                        span,
                    );
                    p.consume_header_comment();
                } else if p.at(TokenKind::Client) {
                    if has_client {
                        p.error_unexpected_token("Duplicate 'client' field".to_string());
                    }
                    has_client = true;
                    p.parse_client_field();
                } else if p.at(TokenKind::Word)
                    && p.current().map(|t| t.text == "prompt").unwrap_or(false)
                {
                    if has_prompt {
                        p.error_unexpected_token("Duplicate 'prompt' field".to_string());
                    }
                    has_prompt = true;
                    p.parse_prompt_field();
                } else {
                    // Unexpected token in LLM function
                    p.error_unexpected_token(format!(
                        "Only 'client' and 'prompt' allowed in LLM function, found '{}'",
                        p.current().map(|t| t.text.as_str()).unwrap_or("EOF")
                    ));
                    p.bump();
                }
            }

            if !has_client {
                p.error_unexpected_token("LLM function missing 'client' field".to_string());
            }
            if !has_prompt {
                p.error_unexpected_token("LLM function missing 'prompt' field".to_string());
            }

            p.expect(TokenKind::RBrace);
        });
    }

    fn parse_expr_function_body(&mut self) {
        self.with_node(SyntaxKind::EXPR_FUNCTION_BODY, |p| {
            p.parse_block_expr();
        });
    }

    fn parse_client_field(&mut self) {
        self.with_node(SyntaxKind::CLIENT_FIELD, |p| {
            p.expect(TokenKind::Client);

            // Client name can be:
            // - A simple identifier: MyClient
            // - A quoted string: "openai/gpt-4o"
            // - An unquoted shorthand: openai/gpt-4o-mini (contains slashes)
            if p.at(TokenKind::Quote) {
                p.parse_string();
            } else if p.at(TokenKind::Word) {
                // Parse unquoted client value - consume tokens until newline or brace
                // This handles cases like: openai/gpt-4o-mini
                while !p.at_end() {
                    if p.at(TokenKind::RBrace) || p.at(TokenKind::LBrace) || p.has_newline_ahead() {
                        break;
                    }
                    p.bump();
                }
            } else {
                p.error_unexpected_token("client name".to_string());
            }
        });
    }

    fn parse_prompt_field(&mut self) {
        self.with_node(SyntaxKind::PROMPT_FIELD, |p| {
            // Expect 'prompt' keyword (as Word token)
            if p.at(TokenKind::Word) && p.current().map(|t| t.text == "prompt").unwrap_or(false) {
                p.bump();
            } else {
                p.error_unexpected_token("'prompt' keyword".to_string());
            }

            // Prompt value (usually a raw string)
            if !p.parse_any_string() {
                p.error_unexpected_token("prompt string".to_string());
            }
        });
    }

    /// Parse a block expression with statements
    fn parse_block_expr(&mut self) {
        self.with_node(SyntaxKind::BLOCK_EXPR, |p| {
            p.expect(TokenKind::LBrace);

            // Parse statements until closing brace
            while !p.at(TokenKind::RBrace) && !p.at_end() {
                // Error recovery: if we see a top-level keyword, assume we missed a closing brace
                if p.at_top_level_keyword() {
                    break;
                }

                // Handle MDX-style header comments (//#...)
                if p.at_header_comment_start() {
                    p.consume_header_comment();
                    continue;
                }

                p.parse_stmt();
            }

            p.expect(TokenKind::RBrace);
        });
    }

    // ============ Statement Parsing ============

    /// Parse a statement
    fn parse_stmt(&mut self) {
        // Skip stray semicolons
        if self.eat(TokenKind::Semicolon) {
            return;
        }

        if self.at(TokenKind::Watch) {
            self.parse_watch_let_stmt();
        } else if self.at(TokenKind::Let) {
            self.parse_let_stmt();
        } else if self.at(TokenKind::Return) {
            self.parse_return_stmt();
        } else if self.at(TokenKind::While) {
            self.parse_while_stmt();
        } else if self.at(TokenKind::For) {
            self.parse_for_expr();
        } else if self.at(TokenKind::Break) {
            self.parse_break_stmt();
        } else if self.at(TokenKind::Continue) {
            self.parse_continue_stmt();
        } else if self.at(TokenKind::Assert) {
            self.parse_assert_stmt();
        } else {
            // Expression statement
            self.parse_expr_stmt();
        }
    }

    fn parse_let_stmt(&mut self) {
        self.with_node(SyntaxKind::LET_STMT, |p| {
            p.expect(TokenKind::Let);

            // Variable name
            if p.at(TokenKind::Word) {
                p.bump();
            } else {
                p.error_unexpected_token("variable name".to_string());
            }

            // Optional type annotation
            if p.eat(TokenKind::Colon) {
                p.parse_type();
            }

            // Initializer
            if p.eat(TokenKind::Equals) {
                // Parse expression but exclude assignment operators (parse_expr_bp with min_bp=3)
                // This prevents `let a = b = c` from being parsed as nested assignment
                p.parse_expr_bp(3);
            } else {
                p.error_unexpected_token("initializer (=)".to_string());
            }

            // Consume trailing semicolon
            p.eat(TokenKind::Semicolon);
        });
    }

    fn parse_watch_let_stmt(&mut self) {
        self.with_node(SyntaxKind::WATCH_LET, |p| {
            p.expect(TokenKind::Watch);
            p.expect(TokenKind::Let);

            // Variable name
            if p.at(TokenKind::Word) {
                p.bump();
            } else {
                p.error_unexpected_token("variable name".to_string());
            }

            // Optional type annotation
            if p.eat(TokenKind::Colon) {
                p.parse_type();
            }

            // Initializer
            if p.eat(TokenKind::Equals) {
                p.parse_expr_bp(3);
            } else {
                p.error_unexpected_token("initializer (=)".to_string());
            }

            // Consume trailing semicolon
            p.eat(TokenKind::Semicolon);
        });
    }

    fn parse_return_stmt(&mut self) {
        self.with_node(SyntaxKind::RETURN_STMT, |p| {
            p.expect(TokenKind::Return);

            // Optional return value
            if !p.at(TokenKind::RBrace) && !p.at_end() {
                p.parse_expr();
            }

            // Consume trailing semicolon
            p.eat(TokenKind::Semicolon);
        });
    }

    fn parse_assert_stmt(&mut self) {
        self.with_node(SyntaxKind::ASSERT_STMT, |p| {
            p.expect(TokenKind::Assert);

            // Condition expression
            p.parse_expr();

            // Consume trailing semicolon
            p.eat(TokenKind::Semicolon);
        });
    }

    fn parse_if_expr(&mut self) {
        self.with_node(SyntaxKind::IF_EXPR, |p| {
            p.expect(TokenKind::If);

            // Condition
            p.parse_expr();

            // Then block
            if p.at(TokenKind::LBrace) {
                p.parse_block_expr();
            } else {
                p.error_unexpected_token("block after if condition".to_string());
            }

            // Optional else
            if p.at(TokenKind::Else) {
                p.bump(); // else

                if p.at(TokenKind::If) {
                    // else if
                    p.parse_if_expr();
                } else if p.at(TokenKind::LBrace) {
                    // else block
                    p.parse_block_expr();
                } else {
                    p.error_unexpected_token("'if' or block after 'else'".to_string());
                }
            }
        });
    }

    /// Parse a match expression.
    ///
    /// Grammar (from BEP-002):
    /// ```text
    /// match_expr := 'match' '(' expr ')' '{' match_arm+ '}'
    /// ```
    fn parse_match_expr(&mut self) {
        self.with_node(SyntaxKind::MATCH_EXPR, |p| {
            p.expect(TokenKind::Match);

            // Scrutinee expression in parentheses
            if p.at(TokenKind::LParen) {
                p.bump(); // (
                p.parse_expr();
                p.expect(TokenKind::RParen);
            } else {
                p.error_unexpected_token("'(' after 'match'".to_string());
            }

            // Match body with arms
            if p.at(TokenKind::LBrace) {
                p.bump(); // {

                // Parse at least one arm
                if !p.at(TokenKind::RBrace) {
                    p.parse_match_arm();

                    // Parse additional arms
                    while !p.at(TokenKind::RBrace) && !p.at_end() {
                        // Error recovery: if we see a top-level keyword, assume we missed a closing brace
                        if p.at_top_level_keyword() {
                            break;
                        }
                        p.parse_match_arm();
                    }
                } else {
                    p.error_unexpected_token("at least one match arm".to_string());
                }

                p.expect(TokenKind::RBrace);
            } else {
                p.error_unexpected_token("'{' after match scrutinee".to_string());
            }
        });
    }

    /// Parse a single match arm.
    ///
    /// Grammar (from BEP-002):
    /// ```text
    /// match_arm := pattern guard? '=>' arm_body
    /// guard     := 'if' expr
    /// arm_body  := expr | block_expr
    /// ```
    fn parse_match_arm(&mut self) {
        self.with_node(SyntaxKind::MATCH_ARM, |p| {
            // Parse the pattern
            p.parse_match_pattern();

            // Optional guard: if expr
            if p.at(TokenKind::If) {
                p.parse_match_guard();
            }

            // Expect fat arrow
            if p.at(TokenKind::FatArrow) {
                p.bump(); // =>
            } else {
                p.error_unexpected_token("'=>' after pattern".to_string());
            }

            // Arm body: expression or block
            if p.at(TokenKind::LBrace) {
                p.parse_block_expr();
            } else {
                p.parse_expr();
            }

            // Optional trailing comma
            p.eat(TokenKind::Comma);
        });
    }

    /// Parse a match pattern.
    ///
    /// Grammar (from BEP-002):
    ///
    /// ```text
    /// pattern         := binding_pattern | literal_pattern | union_pattern
    /// binding_pattern := IDENT (':' type_expr)?
    /// literal_pattern := 'null' | 'true' | 'false' | INTEGER | FLOAT | STRING
    /// union_pattern   := (literal_pattern | enum_variant) ('|' (literal_pattern | enum_variant))*
    /// enum_variant    := IDENT '.' IDENT
    /// ```
    ///
    /// Note: `_` is parsed as a regular identifier (binding pattern) - semantic
    /// analysis will treat it as a wildcard/discard.
    fn parse_match_pattern(&mut self) {
        self.with_node(SyntaxKind::MATCH_PATTERN, |p| {
            // First, parse the initial pattern element
            p.parse_pattern_element();

            // Check for union pattern: element | element | ...
            while p.at(TokenKind::Pipe) {
                p.bump(); // |
                p.parse_pattern_element();
            }
        });
    }

    /// Parse a single pattern element (used in patterns and union patterns).
    ///
    /// This can be:
    /// - A literal: null, true, false, integer, float, string
    /// - An enum variant: Ident.Ident
    /// - A binding: ident or ident: Type
    /// - A parenthesized pattern group: (pattern)
    fn parse_pattern_element(&mut self) {
        // Handle parenthesized pattern group (for nested unions like `200 | (201 | 202)`)
        if self.at(TokenKind::LParen) {
            self.bump(); // (
            self.parse_match_pattern(); // Recursive - creates nested MATCH_PATTERN
            self.expect(TokenKind::RParen);
            return;
        }

        // Check for literals first (including negative literals)
        // Handle negative numeric literals: -42, -3.14
        if self.at(TokenKind::Minus) {
            // Peek ahead to see if this is a negative numeric literal
            let is_negative_number = self.peek(1).is_some_and(|t| {
                matches!(t.kind, TokenKind::IntegerLiteral | TokenKind::FloatLiteral)
            });
            if is_negative_number {
                self.bump(); // consume the minus
                self.bump(); // consume the number
            } else {
                self.error_unexpected_token("pattern".to_string());
                self.bump();
            }
        } else if self.at(TokenKind::IntegerLiteral) || self.at(TokenKind::FloatLiteral) {
            self.bump();
        } else if self.parse_any_string() {
            // String literal handled
        } else if self.at(TokenKind::Word) {
            let text = self.current().map(|t| t.text.as_str()).unwrap_or("");

            if text == "null" || text == "true" || text == "false" {
                // Literal keywords
                self.bump();
            } else {
                // Could be:
                // 1. Enum variant: Ident.Ident (e.g., Status.Active)
                // 2. Binding without type: ident (including _ as wildcard)
                // 3. Binding with type: ident: Type

                self.bump(); // First identifier

                if self.at(TokenKind::Dot) {
                    // Enum variant pattern: Ident.Ident
                    self.bump(); // .
                    if self.at(TokenKind::Word) {
                        self.bump(); // variant name
                    } else {
                        self.error_unexpected_token("enum variant name after '.'".to_string());
                    }
                } else if self.at(TokenKind::Colon) {
                    // Typed binding pattern: ident: Type
                    self.bump(); // :
                    self.parse_type();
                }
                // else: simple binding pattern (just the identifier)
            }
        } else {
            self.error_unexpected_token("pattern".to_string());
            // Consume unexpected token to avoid infinite loop
            if !self.at_end() {
                self.bump();
            }
        }
    }

    /// Parse a match guard.
    ///
    /// Grammar: guard := 'if' expr
    fn parse_match_guard(&mut self) {
        self.with_node(SyntaxKind::MATCH_GUARD, |p| {
            p.expect(TokenKind::If);
            p.parse_expr();
        });
    }

    fn parse_while_stmt(&mut self) {
        self.with_node(SyntaxKind::WHILE_STMT, |p| {
            p.expect(TokenKind::While);

            // Condition
            p.parse_expr();

            // Body
            if p.at(TokenKind::LBrace) {
                p.parse_block_expr();
            } else {
                p.error_unexpected_token("block after while condition".to_string());
            }
        });
    }

    fn parse_for_expr(&mut self) {
        self.with_node(SyntaxKind::FOR_EXPR, |p| {
            p.expect(TokenKind::For);

            // Check for parenthesized form: for (...) { }
            if p.at(TokenKind::LParen) {
                p.bump(); // (

                // Check if this is iterator-style: for (let var in expr) or C-style: for (init; cond; update)
                if p.at(TokenKind::Let) {
                    // Peek ahead to check if this is iterator-style (has 'in' keyword)
                    // For iterator-style: for (let i in expr)
                    // For C-style: for (let i = 0; ...)
                    if p.looks_like_for_in_loop() {
                        // Iterator-style: for (let var in expr)
                        p.parse_for_in_pattern();
                        p.expect(TokenKind::In);
                        p.parse_expr(); // iterator expression
                    } else {
                        // C-style: for (let i = 0; cond; update)
                        p.parse_let_stmt();
                        // The let statement already consumed the semicolon
                        // Now parse condition
                        if !p.at(TokenKind::Semicolon) && !p.at(TokenKind::RParen) {
                            p.parse_expr(); // condition
                        }
                        p.eat(TokenKind::Semicolon);

                        // Parse update expression
                        if !p.at(TokenKind::RParen) {
                            p.parse_expr(); // update
                        }
                    }
                } else if p.at(TokenKind::Word) {
                    // Could be iterator-style: for (i in expr)
                    // Or could be C-style starting with expression: for (i = 0; ...)
                    // Look ahead to determine
                    if p.peek(1).map(|t| t.kind == TokenKind::In).unwrap_or(false) {
                        // Simple iterator-style without let: for (i in expr)
                        p.bump(); // variable name
                        p.bump(); // in
                        p.parse_expr(); // iterator expression
                    } else {
                        // C-style without initializer starting with expression
                        // Just parse as expression-based C-style
                        p.parse_c_style_for_body();
                    }
                } else if p.at(TokenKind::Semicolon) {
                    // C-style with empty initializer: for (; cond; update)
                    p.parse_c_style_for_body();
                } else {
                    p.error_unexpected_token("loop variable, 'let', or ';'".to_string());
                }

                p.expect(TokenKind::RParen);
            } else {
                // Non-parenthesized form: for var in expr { }
                if p.at(TokenKind::Word) {
                    p.bump();
                } else {
                    p.error_unexpected_token("loop variable".to_string());
                }

                p.expect(TokenKind::In);
                p.parse_expr();
            }

            // Body
            if p.at(TokenKind::LBrace) {
                p.parse_block_expr();
            } else {
                p.error_unexpected_token("block after for expression".to_string());
            }
        });
    }

    /// Check if this looks like a for-in loop (has 'in' keyword after variable name)
    fn looks_like_for_in_loop(&self) -> bool {
        // We're at 'let', look for pattern: let WORD in
        // Skip: let (0), WORD (1), check for 'in' (2)
        self.peek(2)
            .map(|t| t.kind == TokenKind::In)
            .unwrap_or(false)
    }

    /// Parse C-style for loop body (condition and update parts): ; cond; update
    /// Called when we've already consumed any initializer or are at the first semicolon.
    fn parse_c_style_for_body(&mut self) {
        // Consume first semicolon (separates initializer from condition)
        self.eat(TokenKind::Semicolon);

        // Parse condition expression (if present)
        if !self.at(TokenKind::Semicolon) && !self.at(TokenKind::RParen) {
            self.parse_expr();
        }

        // Consume second semicolon (separates condition from update)
        self.eat(TokenKind::Semicolon);

        // Parse update expression (if present)
        if !self.at(TokenKind::RParen) {
            self.parse_expr();
        }
    }

    /// Parse a for-in loop pattern: let var (without initializer)
    fn parse_for_in_pattern(&mut self) {
        self.with_node(SyntaxKind::LET_STMT, |p| {
            p.expect(TokenKind::Let);

            // Variable name
            if p.at(TokenKind::Word) {
                p.bump();
            } else {
                p.error_unexpected_token("variable name".to_string());
            }

            // Optional type annotation
            if p.eat(TokenKind::Colon) {
                p.parse_type();
            }

            // No initializer for for-in loops - don't emit error
        });
    }

    fn parse_break_stmt(&mut self) {
        self.with_node(SyntaxKind::BREAK_STMT, |p| {
            p.expect(TokenKind::Break);
            p.eat(TokenKind::Semicolon);
        });
    }

    fn parse_continue_stmt(&mut self) {
        self.with_node(SyntaxKind::CONTINUE_STMT, |p| {
            p.expect(TokenKind::Continue);
            p.eat(TokenKind::Semicolon);
        });
    }

    fn parse_expr_stmt(&mut self) {
        // Just an expression followed by optional semicolon
        self.parse_expr();
        self.eat(TokenKind::Semicolon); // Optional semicolon
    }

    // ============ Expression Parsing (Pratt Parser) ============

    /// Parse an expression with operator precedence
    fn parse_expr(&mut self) {
        self.parse_expr_bp(0);
    }

    /// Parse expression with binding power (Pratt parsing)
    fn parse_expr_bp(&mut self, min_bp: u8) {
        // Mark the start of this expression to prevent wrapping earlier tokens
        let expr_start = self.events.len();

        // Parse prefix (primary expression or unary operator)
        self.parse_prefix();

        // Parse infix operators and postfix operations
        while let Some(token) = self.current() {
            let op = token.kind;

            // If we see a / that might be the start of a header comment, check and stop
            // Headers should only appear at statement boundaries, not in expressions
            if op == TokenKind::Slash {
                // Check if this is the start of a header comment (//#)
                // We need to check the raw token stream, not current() which skips comments
                if self.at_header_comment_start() {
                    break;
                }
            }

            // Check for special cases first
            if op == TokenKind::Less && self.looks_like_generic_args() {
                // Parse as generic arguments: foo<T>
                let lhs_start = self.find_previous_expr_start_after(expr_start);
                self.wrap_events_in_node(lhs_start, SyntaxKind::PATH_EXPR);
                self.parse_generic_args();
                self.finish_node();
                // Continue to potentially parse function call
                continue;
            } else if op == TokenKind::LParen {
                // Function call
                let lhs_start = self.find_previous_expr_start_after(expr_start);
                self.wrap_events_in_node(lhs_start, SyntaxKind::CALL_EXPR);
                self.parse_call_args();
                self.finish_node();
            } else if op == TokenKind::LBracket {
                // Index expression
                let lhs_start = self.find_previous_expr_start_after(expr_start);
                self.wrap_events_in_node(lhs_start, SyntaxKind::INDEX_EXPR);
                self.bump(); // [
                self.parse_expr();
                self.expect(TokenKind::RBracket);
                self.finish_node();
            } else if op == TokenKind::Dot || op == TokenKind::Dollar {
                // Field access on a complex expression.
                //
                // This branch handles `.field` when the base is already a complete
                // expression (call, index, binary, etc.):
                // - `f().field` -> FIELD_ACCESS_EXPR wrapping CALL_EXPR
                // - `arr[0].field` -> FIELD_ACCESS_EXPR wrapping INDEX_EXPR
                // - `(a + b).field` -> FIELD_ACCESS_EXPR wrapping PAREN_EXPR
                //
                // For simple identifier chains like `user.name.length`, the parser
                // uses PATH_EXPR instead (see `parse_path_or_ident`). PATH_EXPR is
                // created during primary expression parsing when we see `WORD.WORD`.
                //
                // Also handles special `.$field` syntax for watch variables.
                let lhs_start = self.find_previous_expr_start_after(expr_start);
                self.wrap_events_in_node(lhs_start, SyntaxKind::FIELD_ACCESS_EXPR);
                self.bump(); // . or $
                if self.at(TokenKind::Word) {
                    self.bump();
                } else {
                    let punct = if op == TokenKind::Dollar {
                        "'$'"
                    } else {
                        "'.'"
                    };
                    self.error_unexpected_token(format!("Expected field name after {punct}"));
                }
                self.finish_node();
            } else if op == TokenKind::LBrace {
                // Object literal/constructor
                // Check if we have a preceding expression (constructor name/expression)
                // by checking if we've emitted any events since expr_start
                if self.events.len() > expr_start && self.looks_like_object_constructor() {
                    // We have a preceding expression that looks like a type/constructor,
                    // treat as object literal/constructor
                    let lhs_start = self.find_previous_expr_start_after(expr_start);
                    self.wrap_events_in_node(lhs_start, SyntaxKind::OBJECT_LITERAL);
                    self.parse_object_literal_body();
                    self.finish_node();
                } else {
                    // No preceding expression, or preceding expression doesn't look like
                    // a constructor (e.g., it's a literal or binary expression)
                    // Don't consume the brace - it's likely a block/body for an outer construct
                    break;
                }
            } else if let Some((left_bp, right_bp)) = Self::infix_binding_power(op) {
                // General infix operators (including < when it's not generic args)
                if left_bp < min_bp {
                    break;
                }

                // Mark where to start wrapping (before the LHS we just parsed)
                // but not before the expr_start marker
                let lhs_start = self.find_previous_expr_start_after(expr_start);

                // Consume the operator
                self.bump();

                // Parse right-hand side
                self.parse_expr_bp(right_bp);

                // Wrap everything from lhs_start in a BINARY_EXPR
                self.wrap_events_in_node(lhs_start, SyntaxKind::BINARY_EXPR);
                self.finish_node();
            } else {
                break;
            }
        }
    }

    /// Find the start of the most recent complete expression, but not before `min_index`
    /// This walks backward through events to find where the last expression began
    fn find_previous_expr_start_after(&self, min_index: usize) -> usize {
        let mut depth = 0;
        let mut i = self.events.len();

        while i > min_index {
            i -= 1;
            match &self.events[i] {
                Event::FinishNode => depth += 1,
                Event::StartNode { .. } => {
                    if depth == 0 {
                        return i;
                    }
                    depth -= 1;
                }
                Event::Token { .. } => {
                    if depth == 0 {
                        return i;
                    }
                }
                Event::UnexpectedToken { .. } | Event::SyntaxHint { .. } => {}
            }
        }

        min_index
    }

    /// Check if the most recent expression looks like a constructor/type name
    /// that can be followed by `{` for object literal construction.
    ///
    /// Returns true for:
    /// - Simple identifiers (e.g., `Point`)
    /// - Path expressions (e.g., `module.Type` for future module support)
    ///
    /// Returns false for everything else:
    /// - Literals (e.g., `18`, `"string"`)
    /// - Binary expressions (e.g., `a < b`)
    /// - Function calls (e.g., `func()`)
    /// - Any other complex expression
    fn looks_like_object_constructor(&self) -> bool {
        // Walk backward to find the most recent complete expression
        let mut depth = 0;
        for event in self.events.iter().rev() {
            match event {
                Event::FinishNode => depth += 1,
                Event::StartNode { kind } => {
                    depth -= 1;
                    if depth == 0 {
                        // We just closed a complete expression
                        // Allow PATH_EXPR or FIELD_ACCESS_EXPR for module-qualified types
                        return matches!(
                            kind,
                            SyntaxKind::PATH_EXPR | SyntaxKind::FIELD_ACCESS_EXPR
                        );
                    }
                }
                Event::Token { kind, .. } => {
                    if depth == 0 {
                        // The most recent thing is a bare token (no wrapping node)
                        // Only WORD tokens can be type names
                        return *kind == SyntaxKind::WORD;
                    }
                }
                Event::UnexpectedToken { .. } | Event::SyntaxHint { .. } => {}
            }
        }
        false
    }

    /// Wrap events from `start_index` onwards in a new node
    /// This allows us to retroactively wrap parsed expressions.
    ///
    /// For example, in an expression like `a + b`, the parser will
    /// parse `a` before seeing the binary operator that triggers
    /// binary expression parsing, so we need this function to
    /// reassociate the event from that previous expression into
    /// the binary expression node.
    fn wrap_events_in_node(&mut self, start_index: usize, kind: SyntaxKind) {
        // Insert StartNode at the beginning
        self.events.insert(start_index, Event::StartNode { kind });
    }

    /// Parse prefix expression (primary or unary operator)
    fn parse_prefix(&mut self) {
        // Check for unary operators
        if self.at(TokenKind::Minus)
            || self.at(TokenKind::Not)
            || self.at(TokenKind::Tilde)
            || self.at(TokenKind::PlusPlus)
            || self.at(TokenKind::MinusMinus)
        {
            self.with_node(SyntaxKind::UNARY_EXPR, |p| {
                p.bump(); // operator
                p.parse_prefix(); // operand
            });
        } else {
            self.parse_primary_expr();
        }
    }

    /// Parse primary expression (literals, identifiers, parentheses)
    fn parse_primary_expr(&mut self) {
        if self.at(TokenKind::IntegerLiteral) || self.at(TokenKind::FloatLiteral) {
            // Numeric literal
            self.bump();
        } else if self.parse_any_string() {
            // String literal
        } else if self.at(TokenKind::Word) {
            let text = self.current().map(|t| t.text.as_str()).unwrap_or("");
            if text == "true" || text == "false" {
                // Boolean literal
                self.bump();
            } else if text == "null" {
                // Null literal
                self.bump();
            } else {
                // Identifier or path (could be multi-segment like baml.HttpMethod.Get)
                self.parse_path_or_ident();
            }
        } else if self.at(TokenKind::LParen) {
            // Parenthesized expression
            self.with_node(SyntaxKind::PAREN_EXPR, |p| {
                p.bump(); // (
                p.parse_expr();
                p.expect(TokenKind::RParen);
            });
        } else if self.at(TokenKind::LBracket) {
            // Array literal
            self.parse_array_literal();
        } else if self.at(TokenKind::LBrace) {
            // Could be block expression or map literal
            // Peek ahead to determine which one
            if self.looks_like_map() {
                // Map literal: { "key": value, ... }
                self.parse_map_literal();
            } else {
                // Block expression: { statements... }
                self.parse_block_expr();
            }
        } else if self.at(TokenKind::If) {
            // If expression (can be used in expression context like `let x = if (cond) { a } else { b }`)
            self.parse_if_expr();
        } else if self.at(TokenKind::Match) {
            // Match expression
            self.parse_match_expr();
        } else {
            self.error_unexpected_token("expression".to_string());
            // Consume the unexpected token to avoid infinite loops
            if !self.at_end() {
                self.bump();
            }
        }
    }

    fn parse_call_args(&mut self) {
        self.with_node(SyntaxKind::CALL_ARGS, |p| {
            p.expect(TokenKind::LParen);

            if !p.at(TokenKind::RParen) {
                p.parse_expr();

                while p.eat(TokenKind::Comma) {
                    if p.at(TokenKind::RParen) {
                        break; // Trailing comma
                    }
                    p.parse_expr();
                }
            }

            p.expect(TokenKind::RParen);
        });
    }

    fn parse_array_literal(&mut self) {
        self.with_node(SyntaxKind::ARRAY_LITERAL, |p| {
            p.expect(TokenKind::LBracket);

            if !p.at(TokenKind::RBracket) {
                p.parse_expr();

                // Allow commas and/or newlines as separators between elements
                loop {
                    // Consume optional comma
                    p.eat(TokenKind::Comma);

                    // Check if we're done
                    if p.at(TokenKind::RBracket) || p.at_end() {
                        break;
                    }

                    p.parse_expr();
                }
            }

            p.expect(TokenKind::RBracket);
        });
    }

    /// Check if < starts generic arguments rather than a comparison
    /// Generic args: foo<Type>, foo<A, B>
    /// Comparison: a < b
    fn looks_like_generic_args(&self) -> bool {
        if !self.at(TokenKind::Less) {
            return false;
        }

        // Look ahead to see if it's a type name followed by > or ,
        if let Some(token_after_less) = self.peek(1) {
            // Must be a word (type name)
            if token_after_less.kind == TokenKind::Word {
                // Check what comes after the word
                if let Some(token_after_word) = self.peek(2) {
                    // Generic args end with > or have comma for multiple args
                    if token_after_word.kind == TokenKind::Greater
                        || token_after_word.kind == TokenKind::Comma
                    {
                        return true;
                    }
                    // Could also have nested generics: Foo<Bar<T>>
                    if token_after_word.kind == TokenKind::Less {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Parse generic arguments: <Type1, Type2, ...>
    fn parse_generic_args(&mut self) {
        self.type_args_depth += 1;
        self.with_node(SyntaxKind::GENERIC_ARGS, |p| {
            p.expect(TokenKind::Less);

            // Parse first type argument
            if !p.at(TokenKind::Greater) {
                p.parse_type();

                // Parse remaining type arguments
                while p.eat(TokenKind::Comma) {
                    if p.at(TokenKind::Greater) {
                        break; // Trailing comma
                    }
                    p.parse_type();
                }
            }

            p.expect_greater();
        });
        self.type_args_depth -= 1;

        // If we just exited the outermost generic and have pending '>', report error
        if self.type_args_depth == 0 && self.pending_greaters > 0 {
            if let Some(span) = self.pending_greater_span {
                self.error(
                    format!(
                        "Unmatched '>' in type expression (found {} extra)",
                        self.pending_greaters
                    ),
                    span,
                );
            }
            self.pending_greaters = 0;
            self.pending_greater_span = None;
        }
    }

    /// Check if the current position looks like a map literal rather than a block
    /// Maps start with { "string": or { identifier:
    /// Blocks typically start with { keyword or { expression (but not field:value pattern)
    fn looks_like_map(&self) -> bool {
        // Must start with {
        if !self.at(TokenKind::LBrace) {
            return false;
        }

        // Look at the token after {
        if let Some(token_after_brace) = self.peek(1) {
            // Empty braces - treat as empty map
            if token_after_brace.kind == TokenKind::RBrace {
                return true;
            }

            // Check for string literal key
            if token_after_brace.kind == TokenKind::Quote
                || token_after_brace.kind == TokenKind::Hash
            {
                // Likely a map with string key
                return true;
            }

            // Check for identifier followed by colon (map with identifier key)
            if token_after_brace.kind == TokenKind::Word {
                // Check if it's a keyword that starts statements
                let text = &token_after_brace.text;
                if text == "let"
                    || text == "return"
                    || text == "if"
                    || text == "while"
                    || text == "for"
                    || text == "break"
                    || text == "continue"
                {
                    return false; // It's a block with a statement
                }

                // Check if word is followed by colon (map field)
                // Config-style (word value) is only allowed in config contexts, not expressions
                if let Some(token_after_word) = self.peek(2) {
                    if token_after_word.kind == TokenKind::Colon {
                        return true; // word: pattern indicates a map
                    }
                }
            }
        }

        false // Default to block
    }

    /// Parse a map literal in expression context: { "key": value, ... }
    /// Requires colons and commas (JSON-style)
    fn parse_map_literal(&mut self) {
        self.with_node(SyntaxKind::MAP_LITERAL, |p| {
            p.expect(TokenKind::LBrace);

            // Parse map entries
            while !p.at(TokenKind::RBrace) && !p.at_end() {
                // Check for valid entry start
                if p.at(TokenKind::Word) || p.at(TokenKind::Quote) || p.at(TokenKind::Hash) {
                    p.parse_map_entry();

                    // Handle comma between entries
                    if !p.at(TokenKind::RBrace) {
                        if !p.eat(TokenKind::Comma) {
                            // Missing comma - error but try to continue
                            p.error_unexpected_token("',' or '}' after map entry".to_string());
                            // Try to recover
                            if !p.at(TokenKind::Word)
                                && !p.at(TokenKind::Quote)
                                && !p.at(TokenKind::Hash)
                                && !p.at(TokenKind::RBrace)
                            {
                                // Skip unexpected token
                                p.bump();
                            }
                        }
                    }
                } else if p.eat(TokenKind::Comma) {
                    // Trailing comma or double comma - just continue
                    continue;
                } else {
                    // Unexpected token in map
                    p.error_unexpected_token("map key or '}'".to_string());
                    // Skip the unexpected token to avoid getting stuck
                    p.bump();
                }
            }

            p.expect(TokenKind::RBrace);
        });
    }

    /// Parse a path or simple identifier.
    ///
    /// This creates a `PATH_EXPR` for dot-separated identifier chains:
    /// - `user.name.length` -> `PATH_EXPR` with segments `[user, name, length]`
    /// - `baml.HttpMethod.Get` -> `PATH_EXPR` with segments `[baml, HttpMethod, Get]`
    /// - `Status.Active` -> `PATH_EXPR` with segments `[Status, Active]`
    ///
    /// For a simple identifier without dots, no wrapper node is created.
    ///
    /// # `PATH_EXPR` vs `FIELD_ACCESS_EXPR`
    ///
    /// `PATH_EXPR` is used when ALL segments are identifiers (parsed at the start
    /// of an expression). Resolution of what the path refers to happens later in THIR:
    /// - Local variable + field accesses: `user.name`
    /// - Enum variant: `Status.Active`
    /// - Module path: `baml.HttpMethod`
    ///
    /// `FIELD_ACCESS_EXPR` is used when the base is a complex expression that's
    /// already been parsed (call, index, parenthesized, etc.):
    /// - `f().field` -> `FIELD_ACCESS_EXPR` (base is `CALL_EXPR`)
    /// - `arr[0].field` -> `FIELD_ACCESS_EXPR` (base is `INDEX_EXPR`)
    ///
    /// This distinction is made at parse time because we can determine syntactically
    /// whether the base is a simple identifier chain or a complex expression.
    fn parse_path_or_ident(&mut self) {
        if !self.at(TokenKind::Word) {
            return;
        }

        // Check if this looks like a path (word followed by dot and another word)
        if self
            .peek(1)
            .map(|t| t.kind == TokenKind::Dot)
            .unwrap_or(false)
            && self
                .peek(2)
                .map(|t| t.kind == TokenKind::Word)
                .unwrap_or(false)
        {
            // It's a path - all segments are identifiers
            self.with_node(SyntaxKind::PATH_EXPR, |p| {
                p.bump(); // First segment

                // Parse remaining segments
                while p.eat(TokenKind::Dot) {
                    if p.at(TokenKind::Word) {
                        p.bump(); // Next segment
                    } else {
                        p.error_unexpected_token("path segment after '.'".to_string());
                        break;
                    }
                }
            });
        } else {
            // Simple identifier (no dots)
            self.bump();
        }
    }

    /// Parse a single map entry in expression context: key: value
    /// Requires colon between key and value (JSON-style)
    fn parse_map_entry(&mut self) {
        self.with_node(SyntaxKind::OBJECT_FIELD, |p| {
            // Key - can be identifier or string literal
            if p.at(TokenKind::Word) {
                p.bump(); // identifier key
            } else if !p.parse_any_string() {
                p.error_unexpected_token("map key".to_string());
                return;
            }

            // Colon required in expression context
            if !p.expect(TokenKind::Colon) {
                return; // Error already emitted by expect
            }

            // Value - any expression (including nested maps)
            p.parse_expr();
        });
    }

    /// Parse the body of an object literal/constructor: { field: value, ...spread }
    fn parse_object_literal_body(&mut self) {
        self.expect(TokenKind::LBrace);

        // Parse fields until we hit the closing brace
        while !self.at(TokenKind::RBrace) && !self.at_end() {
            // Check for spread element: ...expr
            if self.at(TokenKind::DotDotDot) {
                self.parse_spread_element();

                // Handle comma between elements
                if !self.at(TokenKind::RBrace) {
                    if !self.eat(TokenKind::Comma) {
                        // Missing comma - error but try to continue
                        self.error_unexpected_token("',' or '}' after spread element".to_string());
                        // Try to recover
                        if !self.at(TokenKind::Word)
                            && !self.at(TokenKind::Quote)
                            && !self.at(TokenKind::Hash)
                            && !self.at(TokenKind::DotDotDot)
                            && !self.at(TokenKind::RBrace)
                        {
                            self.bump();
                        }
                    }
                }
            // Check for valid field start
            } else if self.at(TokenKind::Word)
                || self.at(TokenKind::Quote)
                || self.at(TokenKind::Hash)
            {
                self.parse_object_field();

                // Handle comma between fields
                if !self.at(TokenKind::RBrace) {
                    if !self.eat(TokenKind::Comma) {
                        // Missing comma - error but try to continue
                        self.error_unexpected_token("',' or '}' after object field".to_string());
                        // Try to recover by looking for next field or closing brace
                        if !self.at(TokenKind::Word)
                            && !self.at(TokenKind::Quote)
                            && !self.at(TokenKind::Hash)
                            && !self.at(TokenKind::DotDotDot)
                            && !self.at(TokenKind::RBrace)
                        {
                            // Skip unexpected token
                            self.bump();
                        }
                    }
                }
            } else if self.eat(TokenKind::Comma) {
                // Trailing comma or double comma - just continue
                continue;
            } else {
                // Unexpected token in object literal
                self.error_unexpected_token("field name, spread element, or '}'".to_string());
                // Skip the unexpected token to avoid getting stuck
                self.bump();
            }
        }

        self.expect(TokenKind::RBrace);
    }

    /// Parse a spread element: ...expr
    fn parse_spread_element(&mut self) {
        self.with_node(SyntaxKind::SPREAD_ELEMENT, |p| {
            p.expect(TokenKind::DotDotDot);
            p.parse_expr();
        });
    }

    /// Parse a single object field: name: value
    fn parse_object_field(&mut self) {
        self.with_node(SyntaxKind::OBJECT_FIELD, |p| {
            // Field name - can be identifier or string literal
            if p.at(TokenKind::Word) {
                p.bump(); // identifier field name
            } else if !p.parse_any_string() {
                p.error_unexpected_token("field name".to_string());
                return;
            }

            // Colon
            if !p.expect(TokenKind::Colon) {
                return; // Error already emitted by expect
            }

            // Field value - any expression (including nested constructors)
            p.parse_expr();
        });
    }

    /// Get infix operator binding power (precedence)
    /// Returns (`left_bp`, `right_bp`) for left and right associativity
    fn infix_binding_power(op: TokenKind) -> Option<(u8, u8)> {
        use TokenKind::{
            And, AndAnd, AndEquals, Caret, CaretEquals, Equals, EqualsEquals, Greater,
            GreaterEquals, GreaterGreater, GreaterGreaterEquals, Instanceof, Less, LessEquals,
            LessLess, LessLessEquals, Minus, MinusEquals, NotEquals, OrOr, Percent, PercentEquals,
            Pipe, PipeEquals, Plus, PlusEquals, Slash, SlashEquals, Star, StarEquals,
        };

        Some(match op {
            // Assignment operators (right associative)
            Equals | PlusEquals | MinusEquals | StarEquals | SlashEquals | PercentEquals
            | AndEquals | PipeEquals | CaretEquals | LessLessEquals | GreaterGreaterEquals => {
                (2, 1)
            }

            // Logical OR (left associative)
            OrOr => (3, 4),

            // Logical AND (left associative)
            AndAnd => (5, 6),

            // Bitwise OR (left associative)
            Pipe => (7, 8),

            // Bitwise XOR (left associative)
            Caret => (9, 10),

            // Bitwise AND (left associative)
            And => (11, 12),

            // Equality (left associative)
            EqualsEquals | NotEquals => (13, 14),

            // Comparison (left associative) - includes instanceof
            Less | Greater | LessEquals | GreaterEquals | Instanceof => (15, 16),

            // Bitwise shift (left associative)
            LessLess | GreaterGreater => (17, 18),

            // Addition/Subtraction (left associative)
            Plus | Minus => (19, 20),

            // Multiplication/Division/Modulo (left associative)
            Star | Slash | Percent => (21, 22),

            _ => return None,
        })
    }

    // ============ Client Parsing ============

    /// Parse a client declaration
    pub(crate) fn parse_client(&mut self) {
        self.with_node(SyntaxKind::CLIENT_DEF, |p| {
            // 'client' keyword
            p.expect(TokenKind::Client);

            // Optional client type: <llm>
            if p.at(TokenKind::Less) {
                p.with_node(SyntaxKind::CLIENT_TYPE, |p| {
                    p.bump(); // <
                    if p.at(TokenKind::Word) {
                        p.bump(); // type name
                    }
                    p.expect_greater(); // >
                });
            }

            // Client name
            if p.at(TokenKind::Word) {
                p.bump();
            } else {
                p.error_unexpected_token("client name".to_string());
            }

            // Config block
            if p.at(TokenKind::LBrace) {
                p.parse_config_block();
            } else {
                p.error_unexpected_token("config block".to_string());
            }
        });
    }

    fn parse_config_block(&mut self) {
        self.with_node(SyntaxKind::CONFIG_BLOCK, |p| {
            p.expect(TokenKind::LBrace);

            while !p.at(TokenKind::RBrace) && !p.at_end() {
                // Error recovery: if we see a top-level keyword, assume we missed a closing brace.
                // Exceptions - these keywords can appear as config keys:
                // - RetryPolicy: `retry_policy MyPolicy` inside client blocks
                // - TypeBuilder: `type_builder { ... }` inside test blocks
                // - Dynamic: `dynamic class Foo { ... }` inside type_builder blocks
                // - Enum: `enum ["celsius", "fahrenheit"]` inside nested option maps
                // - Class: `class "MyClass"` inside nested option maps
                if p.at_top_level_keyword()
                    && !p.at(TokenKind::RetryPolicy)
                    && !p.at(TokenKind::TypeBuilder)
                    && !p.at(TokenKind::Dynamic)
                    && !p.at(TokenKind::Enum)
                    && !p.at(TokenKind::Class)
                {
                    break;
                }

                // Block attributes like @@check(...) inside config blocks
                if p.at(TokenKind::AtAt) {
                    p.parse_block_attribute();
                } else {
                    p.parse_config_item();
                }
                // Allow optional comma between config items
                p.eat(TokenKind::Comma);
            }

            p.expect(TokenKind::RBrace);
        });
    }

    fn parse_config_item(&mut self) {
        // Special handling for type_builder blocks inside test definitions
        if self.at(TokenKind::TypeBuilder) {
            self.parse_type_builder_block();
            return;
        }

        // Special handling for dynamic type definitions inside type_builder blocks
        if self.at(TokenKind::Dynamic) {
            self.parse_dynamic_type_def();
            return;
        }

        // Note: type_builder blocks handle class/enum declarations in their own loop
        // (see parse_type_builder_block). In regular config blocks, "class" and "enum"
        // should be treated as config keys (e.g., `enum ["celsius", "fahrenheit"]`).

        self.with_node(SyntaxKind::CONFIG_ITEM, |p| {
            // Config key: identifier, keyword-as-identifier, or quoted/raw string
            // Note: Some top-level keywords are also valid as config keys:
            // - RetryPolicy: `retry_policy MyPolicy` inside client blocks
            // - Enum: `enum ["celsius", "fahrenheit"]` inside nested option maps
            // - Class: `class "MyClass"` inside nested option maps
            // We explicitly allow them here so they parse as config items rather than
            // triggering error recovery that would break out of the config block.
            if p.at(TokenKind::Word)
                || p.at(TokenKind::RetryPolicy)
                || p.at(TokenKind::Enum)
                || p.at(TokenKind::Class)
            {
                p.bump();
            } else if p.at(TokenKind::Quote) || p.at(TokenKind::Hash) {
                // Quoted or raw string key (e.g., "string key" or #"raw key"#)
                if !p.parse_any_string() {
                    p.error_unexpected_token("config key".to_string());
                    if !p.at_end() {
                        p.bump();
                    }
                    return;
                }
            } else {
                p.error_unexpected_token("config key".to_string());
                if !p.at_end() {
                    p.bump();
                }
                return;
            }

            // Config value - can be nested block or simple value
            if p.at(TokenKind::LBrace) {
                // Nested config block
                p.parse_config_block();
            } else {
                // Simple value - unquoted string or other expression
                p.parse_config_value();
            }

            // Optional field attributes after config value (e.g., args { ... } @check(...))
            while p.at(TokenKind::At) && !p.at(TokenKind::AtAt) {
                p.parse_field_attribute();
            }
        });
    }

    /// Parse a `type_builder` block inside a test definition.
    /// Contains class, enum, dynamic class, dynamic enum, and type alias definitions.
    fn parse_type_builder_block(&mut self) {
        self.with_node(SyntaxKind::TYPE_BUILDER_BLOCK, |p| {
            p.expect(TokenKind::TypeBuilder);

            if !p.expect(TokenKind::LBrace) {
                return;
            }

            while !p.at(TokenKind::RBrace) && !p.at_end() {
                // Error recovery: if we see a top-level keyword that's not valid in type_builder
                if p.at_top_level_keyword()
                    && !p.at(TokenKind::Class)
                    && !p.at(TokenKind::Enum)
                    && !p.at(TokenKind::Dynamic)
                    && !p.at(TokenKind::TypeBuilder)
                {
                    break;
                }

                if p.at(TokenKind::Dynamic) {
                    p.parse_dynamic_type_def();
                } else if p.at(TokenKind::Class) {
                    p.parse_class();
                } else if p.at(TokenKind::Enum) {
                    p.parse_enum();
                } else if p.at(TokenKind::Word)
                    && p.current().map(|t| t.text == "type").unwrap_or(false)
                {
                    p.parse_type_alias();
                } else {
                    p.error_unexpected_token(
                        "class, enum, dynamic class, dynamic enum, or type alias".to_string(),
                    );
                    p.bump();
                }
            }

            p.expect(TokenKind::RBrace);
        });
    }

    /// Parse a dynamic type definition (dynamic class or dynamic enum).
    fn parse_dynamic_type_def(&mut self) {
        self.with_node(SyntaxKind::DYNAMIC_TYPE_DEF, |p| {
            p.expect(TokenKind::Dynamic);

            if p.at(TokenKind::Class) {
                p.parse_class();
            } else if p.at(TokenKind::Enum) {
                p.parse_enum();
            } else {
                p.error_unexpected_token("class or enum after 'dynamic'".to_string());
            }
        });
    }

    fn parse_config_value(&mut self) {
        self.with_node(SyntaxKind::CONFIG_VALUE, |p| {
            // Config values can be:
            // - Strings: "value"
            // - Raw strings: #"value"#
            // - Arrays: [item1, item2]
            // - Unquoted strings: gpt-4o, env.OPENAI_API_KEY
            // - Numbers: 123, 3.14

            if p.parse_any_string() {
                // String value
                return;
            }

            // Array in config context: uses config-style parsing for nested objects
            if p.at(TokenKind::LBracket) {
                p.parse_config_array();
                return;
            }

            // Nested config block: key { ... }
            if p.at(TokenKind::LBrace) {
                p.parse_config_block();
                return;
            }

            // Parse unquoted string - consume tokens until newline, comma, or brace/bracket
            while !p.at_end() {
                // Check if we should stop - at brace/bracket/comma OR newline is ahead
                if p.at(TokenKind::RBrace)
                    || p.at(TokenKind::LBrace)
                    || p.at(TokenKind::RBracket)
                    || p.at(TokenKind::Comma)
                    || p.has_newline_ahead()
                {
                    break;
                }
                p.bump();
            }
        });
    }

    /// Parse an array in config context - uses config-style parsing for nested objects
    fn parse_config_array(&mut self) {
        self.with_node(SyntaxKind::ARRAY_LITERAL, |p| {
            p.expect(TokenKind::LBracket);

            if !p.at(TokenKind::RBracket) {
                p.parse_config_array_element();

                // Allow commas and/or newlines as separators
                loop {
                    let pos_before = p.current;
                    p.eat(TokenKind::Comma);
                    if p.at(TokenKind::RBracket) || p.at_end() {
                        break;
                    }
                    p.parse_config_array_element();
                    // Safety: break if no progress was made to avoid infinite loop
                    if p.current == pos_before {
                        p.error_unexpected_token("array element".to_string());
                        p.bump();
                    }
                }
            }

            p.expect(TokenKind::RBracket);
        });
    }

    /// Parse an element in a config array - can be a config block or simple value
    fn parse_config_array_element(&mut self) {
        if self.at(TokenKind::LBrace) {
            // Parse as config block (config-style: no colons required)
            self.parse_config_block();
        } else if self.at(TokenKind::RBracket) {
            // Empty or trailing - don't consume
        } else if self.at(TokenKind::Word) {
            // Simple identifier (e.g., client names in strategy arrays)
            self.with_node(SyntaxKind::CONFIG_VALUE, |p| {
                p.bump();
            });
        } else {
            // Parse as simple value (string, number, etc.)
            self.parse_config_value();
        }
    }

    // ============ Test Parsing ============

    /// Parse a test declaration
    pub(crate) fn parse_test(&mut self) {
        self.with_node(SyntaxKind::TEST_DEF, |p| {
            // 'test' keyword
            p.expect(TokenKind::Test);

            // Test name
            let test_name = if p.at(TokenKind::Word) {
                let name = p.current().map(|t| t.text.clone());
                p.bump();
                name
            } else {
                p.error_unexpected_token("test name".to_string());
                None
            };

            // Check for unnecessary parentheses and emit helpful hint
            if p.at(TokenKind::LParen) {
                let name = test_name.as_deref().unwrap_or("Name");
                let start_span = p.current().map(|t| t.span).unwrap();
                p.bump(); // consume (
                let end_span = if p.at(TokenKind::RParen) {
                    let span = p.current().map(|t| t.span).unwrap();
                    p.bump(); // consume )
                    span
                } else {
                    start_span
                };
                let span = baml_base::Span::new(
                    start_span.file_id,
                    TextRange::new(start_span.range.start(), end_span.range.end()),
                );
                p.error(
                    format!("remove parentheses from test name: `test {name}`"),
                    span,
                );
            }

            // Config block
            if p.at(TokenKind::LBrace) {
                p.parse_config_block();
            } else {
                p.error_unexpected_token("test body".to_string());
            }
        });
    }

    // ============ Retry Policy Parsing ============

    /// Parse a retry policy declaration
    pub(crate) fn parse_retry_policy(&mut self) {
        self.with_node(SyntaxKind::RETRY_POLICY_DEF, |p| {
            // 'retry_policy' keyword
            p.expect(TokenKind::RetryPolicy);

            // Policy name
            if p.at(TokenKind::Word) {
                p.bump();
            } else {
                p.error_unexpected_token("retry policy name".to_string());
            }

            // Config block
            if p.at(TokenKind::LBrace) {
                p.parse_config_block();
            } else {
                p.error_unexpected_token("retry policy body".to_string());
            }
        });
    }

    // ============ Generator Parsing ============

    /// Parse a generator declaration
    pub(crate) fn parse_generator(&mut self) {
        self.with_node(SyntaxKind::GENERATOR_DEF, |p| {
            // 'generator' keyword
            p.expect(TokenKind::Generator);

            // Generator name
            if p.at(TokenKind::Word) {
                p.bump();
            } else {
                p.error_unexpected_token("generator name".to_string());
            }

            // Config block
            if p.at(TokenKind::LBrace) {
                p.parse_config_block();
            } else {
                p.error_unexpected_token("generator body".to_string());
            }
        });
    }

    // ============ Template String Parsing ============

    /// Parse a template string declaration
    pub(crate) fn parse_template_string(&mut self) {
        self.with_node(SyntaxKind::TEMPLATE_STRING_DEF, |p| {
            // 'template_string' keyword
            p.expect(TokenKind::TemplateString);

            // Template name
            if p.at(TokenKind::Word) {
                p.bump();
            } else {
                p.error_unexpected_token("template string name".to_string());
            }

            // Optional parameters - only parse if we see '('
            if p.at(TokenKind::LParen) {
                p.parse_parameter_list();
            }

            // Template body (raw string)
            if !p.parse_any_string() {
                p.error_unexpected_token("template string body".to_string());
            }
        });
    }

    // ============ Type Alias Parsing ============

    /// Parse a type alias declaration
    pub(crate) fn parse_type_alias(&mut self) {
        self.with_node(SyntaxKind::TYPE_ALIAS_DEF, |p| {
            // 'type' keyword
            if p.at(TokenKind::Word) && p.current().map(|t| t.text == "type").unwrap_or(false) {
                p.bump();
            } else {
                p.error_unexpected_token("'type' keyword".to_string());
            }

            // Type alias name
            if p.at(TokenKind::Word) {
                p.bump();
            } else {
                p.error_unexpected_token("type alias name".to_string());
            }

            // Equals
            p.expect(TokenKind::Equals);

            // Type definition
            p.parse_type();

            // Optional attributes
            while p.at(TokenKind::At) && !p.at(TokenKind::AtAt) {
                p.parse_field_attribute();
            }
        });
    }
}

/// Parse tokens into a green tree.
///
/// Returns the green tree and any parse errors encountered.
fn parse_impl(tokens: &[Token], cache: Option<&mut NodeCache>) -> (GreenNode, Vec<ParseError>) {
    let mut parser = Parser::new(tokens);

    parser.start_node(SyntaxKind::SOURCE_FILE);

    // Parse top-level declarations
    while !parser.at_end() {
        if parser.at(TokenKind::Enum) {
            parser.parse_enum();
        } else if parser.at(TokenKind::Class) {
            parser.parse_class();
        } else if parser.at(TokenKind::Function) {
            parser.parse_function();
        } else if parser.at(TokenKind::Client) {
            parser.parse_client();
        } else if parser.at(TokenKind::Generator) {
            parser.parse_generator();
        } else if parser.at(TokenKind::Test) {
            parser.parse_test();
        } else if parser.at(TokenKind::RetryPolicy) {
            parser.parse_retry_policy();
        } else if parser.at(TokenKind::TemplateString) {
            parser.parse_template_string();
        } else if parser.at(TokenKind::Word)
            && parser.current().map(|t| t.text == "type").unwrap_or(false)
        {
            parser.parse_type_alias();
        } else if parser.at(TokenKind::Let) {
            parser.parse_let_stmt();
        } else if parser.at_header_comment_start() {
            parser.consume_header_comment();
        } else if parser.try_recover_invalid_block() {
            // Successfully recovered from invalid block like "classs Foo { ... }"
            // Continue parsing
        } else if parser.try_recover_invalid_type_alias() {
            // Successfully recovered from invalid type alias like "typpe Foo = int"
            // Continue parsing
        } else {
            parser.error_unexpected_token("top-level declaration".to_string());
            parser.bump(); // Skip unknown token
        }
    }

    while parser.current < parser.tokens.len() {
        let token = &parser.tokens[parser.current];
        let kind = token_kind_to_syntax_kind(token.kind);
        parser.events.push(Event::Token {
            kind,
            text: token.text.clone(),
        });
        parser.current += 1;
    }

    parser.finish_node();

    parser.build_tree(cache)
}
