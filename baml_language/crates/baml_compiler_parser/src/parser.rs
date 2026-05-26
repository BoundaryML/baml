//! Parser implementation.
//!
//! Implements a recursive descent parser with error recovery.

use baml_base::Span;
use baml_compiler_lexer::{Token, TokenKind};
use baml_compiler_syntax::SyntaxKind;
use rowan::{GreenNode, GreenNodeBuilder, NodeCache, TextSize};
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
        TokenKind::TestSet => SyntaxKind::KW_TESTSET,
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
        TokenKind::Throw => SyntaxKind::KW_THROW,
        TokenKind::Watch => SyntaxKind::KW_WATCH,
        TokenKind::Instanceof => SyntaxKind::KW_INSTANCEOF,
        TokenKind::Is => SyntaxKind::KW_IS,
        TokenKind::Dynamic => SyntaxKind::KW_DYNAMIC,
        TokenKind::Match => SyntaxKind::KW_MATCH,
        TokenKind::Catch => SyntaxKind::KW_CATCH,
        TokenKind::CatchAll => SyntaxKind::KW_CATCH_ALL,
        TokenKind::Throws => SyntaxKind::KW_THROWS,
        TokenKind::Spawn => SyntaxKind::KW_SPAWN,
        TokenKind::Await => SyntaxKind::KW_AWAIT,

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
        TokenKind::DotDot => SyntaxKind::DOT_DOT,
        TokenKind::Dot => SyntaxKind::DOT,
        TokenKind::Dollar => SyntaxKind::DOLLAR,

        // Special operators
        TokenKind::Arrow => SyntaxKind::ARROW,
        TokenKind::FatArrow => SyntaxKind::FAT_ARROW,
        TokenKind::At => SyntaxKind::AT,
        TokenKind::AtAt => SyntaxKind::AT_AT,
        TokenKind::Pipe => SyntaxKind::PIPE,
        TokenKind::QuestionDot => SyntaxKind::QUESTION_DOT,
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

        // Backslash
        TokenKind::Backslash => SyntaxKind::BACKSLASH,

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
    /// Track contexts where postfix `catch` is not allowed to bind.
    /// Managed by [`Self::parse_expr_bp_no_catch`]; prefer that helper over
    /// manually incrementing/decrementing this counter.
    suppress_catch_depth: u32,
    /// Track nesting depth inside testset bodies where `test`/`testset` statements
    /// are valid. Incremented by `parse_testset_body`, checked by `parse_stmt`.
    testset_body_depth: u32,
    /// Track contexts where a postfix `{ … }` must NOT be consumed as an
    /// object-literal constructor. Set while parsing the optional name
    /// expression of `spawn name { body }` so the body's brace stays
    /// available to `parse_spawn_expr`. Counter so nested spawns nest
    /// correctly.
    suppress_object_literal_depth: u32,
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
            suppress_catch_depth: 0,
            testset_body_depth: 0,
            suppress_object_literal_depth: 0,
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

    fn at_end_raw(&self) -> bool {
        self.current_raw().is_none()
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

    /// Check if the current token is a `Word` with the given text.
    /// Used for contextual keywords like `with` that should not be reserved globally.
    fn at_contextual_kw(&self, kw: &str) -> bool {
        self.current()
            .map(|t| t.kind == TokenKind::Word && t.text == kw)
            .unwrap_or(false)
    }

    /// Consume the current `Word("with")` token, re-labelling it as `KW_WITH`
    /// in the syntax tree. Handles leading trivia just like [`Self::bump`].
    fn bump_contextual_with(&mut self) {
        // Emit leading trivia (whitespace, newlines, comments) before the keyword,
        // matching the same pattern as `bump_impl`.
        while self.current < self.tokens.len() {
            if self.at_line_comment_start() {
                self.consume_line_comment();
                continue;
            }
            if self.at_block_comment_start() {
                self.consume_block_comment();
                continue;
            }
            let token = &self.tokens[self.current];
            if self.is_basic_trivia(token.kind) {
                self.events.push(Event::Token {
                    kind: token_kind_to_syntax_kind(token.kind),
                    text: token.text.clone(),
                });
                self.current += 1;
                continue;
            }
            break;
        }
        // Emit the Word("with") token as KW_WITH
        if self.current < self.tokens.len() {
            self.events.push(Event::Token {
                kind: SyntaxKind::KW_WITH,
                text: self.tokens[self.current].text.clone(),
            });
            self.current += 1;
        }
    }

    /// Check if the current token can start a type expression.
    /// Valid type starts: Word (type name), string literal, integer/float literal,
    /// `-` followed by an integer/float literal (negative literal type), `LParen` (tuple).
    fn is_at_type_start(&self) -> bool {
        self.at(TokenKind::Word)
            || self.at(TokenKind::Quote) // string literal type
            || self.at(TokenKind::Hash) // raw string literal type
            || self.at(TokenKind::IntegerLiteral)
            || self.at(TokenKind::FloatLiteral)
            || self.at(TokenKind::LParen) // tuple/parenthesized type
            || (self.at(TokenKind::Minus)
                && matches!(
                    self.peek(1).map(|t| t.kind),
                    Some(TokenKind::IntegerLiteral | TokenKind::FloatLiteral)
                ))
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

    /// Skip trivia and comments starting from an arbitrary token index.
    fn skip_trivia_and_comments_from(&self, mut i: usize) -> usize {
        while i < self.tokens.len() {
            let new_i = self.skip_comment_at(i);
            if new_i != i {
                i = new_i;
                continue;
            }

            if self.is_basic_trivia(self.tokens[i].kind) {
                i += 1;
                continue;
            }

            break;
        }

        i
    }

    fn skip_header_comment_at(&self, mut i: usize) -> usize {
        if !self.is_header_comment_at(i) {
            return i;
        }

        while i < self.tokens.len() && self.tokens[i].kind != TokenKind::Newline {
            i += 1;
        }
        i
    }

    fn previous_non_trivia_span(&self) -> Option<Span> {
        let mut i = self.current;
        while i > 0 {
            i -= 1;
            let token = &self.tokens[i];
            if !self.is_basic_trivia(token.kind) {
                return Some(token.span);
            }
        }
        None
    }

    fn span_from_to(start: Span, end: Span) -> Span {
        if start.file_id != end.file_id {
            return start;
        }
        Span::new(
            start.file_id,
            TextRange::new(start.range.start(), end.range.end()),
        )
    }

    fn parse_default_expr(&mut self) -> Span {
        let start = self.current().map(|token| token.span);
        // Same invariant as let initializers: assignment-like operators do not
        // bind inside parameter default expressions.
        self.parse_expr_bp(3);
        match (start, self.previous_non_trivia_span()) {
            (Some(start), Some(end)) => Self::span_from_to(start, end),
            (Some(start), None) => start,
            (None, Some(end)) => end,
            (None, None) => Span::default(),
        }
    }

    /// Skip a parenthesized argument list starting at `(`, returning the index after it.
    fn skip_parenthesized_from(&self, mut i: usize) -> Option<usize> {
        i = self.skip_trivia_and_comments_from(i);
        if self.tokens.get(i)?.kind != TokenKind::LParen {
            return Some(i);
        }

        let mut depth = 0_u32;
        while i < self.tokens.len() {
            let new_i = self.skip_comment_at(i);
            if new_i != i {
                i = new_i;
                continue;
            }

            let token = &self.tokens[i];
            if self.is_basic_trivia(token.kind) {
                i += 1;
                continue;
            }

            match token.kind {
                TokenKind::LParen => depth += 1,
                TokenKind::RParen => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(i + 1);
                    }
                }
                _ => {}
            }

            i += 1;
        }

        None
    }

    /// Skip one block attribute (`@@foo(...)`) starting at `@@`, returning the index after it.
    fn skip_block_attribute_from(&self, mut i: usize) -> Option<usize> {
        i = self.skip_trivia_and_comments_from(i);
        if self.tokens.get(i)?.kind != TokenKind::AtAt {
            return None;
        }
        i += 1;

        i = self.skip_trivia_and_comments_from(i);
        let name_kind = self.tokens.get(i)?.kind;
        if !matches!(
            name_kind,
            TokenKind::Word | TokenKind::Dynamic | TokenKind::Throws
        ) {
            return None;
        }
        i += 1;

        loop {
            i = self.skip_trivia_and_comments_from(i);
            if self.tokens.get(i).map(|t| t.kind) != Some(TokenKind::Dot) {
                break;
            }
            i += 1;

            i = self.skip_trivia_and_comments_from(i);
            let segment_kind = self.tokens.get(i)?.kind;
            if !matches!(
                segment_kind,
                TokenKind::Word | TokenKind::Dynamic | TokenKind::Throws
            ) {
                return None;
            }
            i += 1;
        }

        i = self.skip_trivia_and_comments_from(i);
        if self.tokens.get(i).map(|t| t.kind) == Some(TokenKind::LParen) {
            i = self.skip_parenthesized_from(i)?;
        }

        Some(i)
    }

    /// Look ahead past any leading block attributes and return the next item keyword.
    fn item_keyword_after_leading_block_attributes(&self) -> Option<TokenKind> {
        let mut i = self.current;

        loop {
            i = self.skip_trivia_and_comments_from(i);
            let token = self.tokens.get(i)?;
            if token.kind != TokenKind::AtAt {
                return Some(token.kind);
            }
            i = self.skip_block_attribute_from(i)?;
        }
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
                    | TokenKind::TestSet
                    | TokenKind::RetryPolicy
                    | TokenKind::TemplateString
                    | TokenKind::TypeBuilder
            )
        )
    }

    /// [`at_top_level_keyword`] minus tokens that are valid in statement position:
    /// - `client` can start `client.method(...)` (parameter named `client`)
    /// - `test` with a string literal is an expression-body test (valid in blocks)
    /// - `testset` with a string literal is a testset declaration (valid in blocks)
    fn at_top_level_keyword_except_client(&self) -> bool {
        if !self.at_top_level_keyword() {
            return false;
        }
        if self.at(TokenKind::Client) {
            return false;
        }
        // Expression-body test/testset are valid inside block expressions
        if (self.at(TokenKind::Test) && self.looks_like_test_expr_body())
            || self.at(TokenKind::TestSet)
        {
            return false;
        }
        true
    }

    fn at_statement_recovery_boundary(&self) -> bool {
        self.at_top_level_keyword_except_client()
            || matches!(
                self.current().map(|t| t.kind),
                Some(
                    TokenKind::Watch
                        | TokenKind::Let
                        | TokenKind::Return
                        | TokenKind::While
                        | TokenKind::For
                        | TokenKind::Break
                        | TokenKind::Continue
                        | TokenKind::Throw
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
        // Emit that pending `>` as a new token.
        if self.pending_greaters > 0 {
            self.pending_greaters -= 1;
            if self.pending_greaters == 0 {
                self.pending_greater_span = None;
            }
            self.events.push(Event::Token {
                kind: SyntaxKind::GREATER,
                text: ">".to_string(),
            });
            return true;
        }

        if self.at(TokenKind::Greater) {
            self.bump();
            true
        } else if self.at(TokenKind::GreaterGreater) {
            // Handle '>>' as two '>':
            // - Consume the '>>' token and splits into two '>' tokens (first `>` is added to tree)
            // - Track that the second '>' is pending for the outer generic
            let span = self.current().map(|t| {
                let mut span = t.span;
                // Span should be on the second `>` token only
                span.range =
                    TextRange::new(span.range.start() + TextSize::from(1), span.range.end());
                span
            });
            self.events.push(Event::Token {
                kind: SyntaxKind::GREATER,
                text: ">".to_string(),
            });
            self.current += 1;
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

    /// Consume token if it is trivia. Returns true if trivia was consumed.
    fn eat_trivia(&mut self) -> bool {
        if self.at_line_comment_start() {
            self.consume_line_comment();
            true
        } else if self.at_block_comment_start() {
            self.consume_block_comment();
            true
        } else if let Some(token) = self.tokens.get(self.current)
            && self.is_basic_trivia(token.kind)
        {
            let kind = token_kind_to_syntax_kind(token.kind);
            self.events.push(Event::Token {
                kind,
                text: token.text.clone(),
            });
            self.current += 1;
            true
        } else {
            false
        }
    }

    /// Eat a basic trivia token (whitespace or newline).
    fn eat_basic_trivia(&mut self) -> bool {
        let Some(token) = self.tokens.get(self.current) else {
            return false;
        };
        let kind = match token.kind {
            TokenKind::Whitespace => SyntaxKind::WHITESPACE,
            TokenKind::Newline => SyntaxKind::NEWLINE,
            _ => return false,
        };
        self.events.push(Event::Token {
            kind,
            text: token.text.clone(),
        });
        self.current += 1;
        true
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
    /// Will skip *leading* trivia, but only basic trivia is allowed internally
    fn count_consecutive_hashes(&self) -> usize {
        let mut count = 0;
        let mut i = self.current;

        // Skip all leading trivia (whitespace, newlines, AND comments)
        while i < self.tokens.len() {
            if self.is_basic_trivia(self.tokens[i].kind) {
                i += 1;
            } else if self.is_line_comment_at(i) || self.is_block_comment_at(i) {
                i = self.skip_comment_at(i);
            } else {
                break;
            }
        }

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

    /// Find the token position after consuming N hashes.
    /// Skips all trivia (whitespace, newlines, and comments) before the first hash,
    /// then only skips basic trivia (whitespace, newlines) between hashes.
    ///
    /// ## Returns
    /// - `None` if a non-hash, non-basic-trivia token is encountered before the number of hashes is reached.
    /// - `None` if the end has been reached
    /// - `Some(i)` with the first non-basic-trivia token after the hashes. Will always be a valid index in [`Self::tokens`].
    fn find_token_after_hashes(&self, hash_count: usize) -> Option<usize> {
        let mut hashes_seen = 0;
        let mut i = self.current;

        // Skip all leading trivia (whitespace, newlines, AND comments)
        while i < self.tokens.len() {
            if self.is_basic_trivia(self.tokens[i].kind) {
                i += 1;
            } else if self.is_line_comment_at(i) || self.is_block_comment_at(i) {
                i = self.skip_comment_at(i);
            } else {
                break;
            }
        }

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

        // before starting the STRING_LITERAL node, handle all leading trivia
        while self.eat_trivia() {}

        self.with_node(SyntaxKind::STRING_LITERAL, |p| {
            p.bump(); // Opening quote

            // Collect all tokens until closing quote.
            // Use at_end_raw / at_raw / bump_raw throughout so that `*/`
            // and `//` inside the string are kept as literal content instead
            // of being mis-recognised as comment delimiters.
            let mut loop_counter = 0;
            while !p.at_end_raw() {
                loop_counter += 1;
                if loop_counter > 100_000 {
                    p.error_unexpected_token("String parsing exceeded iteration limit".to_string());
                    return;
                }

                if p.at_raw(TokenKind::Backslash) {
                    p.bump_raw(); // Consume backslash
                    if p.current < p.tokens.len() {
                        p.bump_raw(); // Consume the escaped character (whatever it is)
                    }
                    continue;
                }

                if p.at_raw(TokenKind::Quote) {
                    p.bump_raw(); // Consume closing quote
                    return;
                }
                p.bump_raw();
            }

            p.error_unexpected_token("Unclosed string literal".to_string());
        });

        true
    }

    /// Parse a byte string literal: `b"..."`.
    ///
    /// The lexer emits `Word("b"), Quote, (content tokens), Quote`.
    /// We check adjacency: the raw token immediately after `Word("b")` must be
    /// `Quote` with no intervening trivia. This distinguishes `b"hello"` (byte
    /// string) from `b "hello"` (identifier `b` followed by a string).
    ///
    /// Must only be called when `self.current()` is `Word("b")`.
    pub(crate) fn parse_byte_string(&mut self) -> bool {
        // Adjacency check: the raw token immediately after `Word("b")` must be
        // `Quote` with no trivia in between.
        let Some(b_index) = self.current_non_trivia_index() else {
            return false;
        };
        if self.tokens.get(b_index + 1).map(|t| t.kind) != Some(TokenKind::Quote) {
            return false;
        }

        // Before starting the node, handle all leading trivia.
        while self.eat_trivia() {}

        self.with_node(SyntaxKind::BYTE_STRING_LITERAL, |p| {
            p.bump_raw(); // Consume the `b` prefix

            p.bump_raw(); // Opening quote

            // Collect all tokens until closing quote (same logic as parse_string).
            let mut loop_counter = 0;
            while !p.at_end_raw() {
                loop_counter += 1;
                if loop_counter > 100_000 {
                    p.error_unexpected_token(
                        "Byte string parsing exceeded iteration limit".to_string(),
                    );
                    return;
                }

                if p.at_raw(TokenKind::Backslash) {
                    p.bump_raw(); // Consume backslash
                    if p.current < p.tokens.len() {
                        p.bump_raw(); // Consume the escaped character (whatever it is)
                    }
                    continue;
                }

                if p.at_raw(TokenKind::Quote) {
                    p.bump_raw(); // Consume closing quote
                    return;
                }

                p.bump_raw();
            }

            p.error_unexpected_token("Unclosed byte string literal".to_string());
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

        // before starting the RAW_STRING_LITERAL node, handle all leading trivia
        while self.eat_trivia() {}

        self.with_node(SyntaxKind::RAW_STRING_LITERAL, |p| {
            // Consume opening hashes
            for _ in 0..opening_hashes {
                p.bump(); // #
            }
            p.bump(); // Opening "

            // Parse raw string content with Jinja template support
            p.parse_raw_string_content(opening_hashes);
        });

        true
    }

    /// Parse the content inside a raw string, recognizing Jinja template constructs
    fn parse_raw_string_content(&mut self, opening_hashes: usize) {
        let mut loop_counter = 0;

        loop {
            loop_counter += 1;
            if loop_counter > 100_000 {
                self.error_unexpected_token(
                    "Raw string parsing exceeded iteration limit".to_string(),
                );
                break;
            }

            if self.at_end_raw() {
                self.error_unexpected_token(format!(
                    "Unclosed raw string (expected \"{}\")",
                    "#".repeat(opening_hashes)
                ));
                break;
            }

            // Check for closing delimiter
            if self.at_raw(TokenKind::Quote) {
                let closing_hashes = self.count_consecutive_hashes_after_quote();
                if closing_hashes == opening_hashes {
                    // Found matching closing delimiter
                    self.bump_raw(); // Closing "
                    for _ in 0..closing_hashes {
                        self.bump_raw(); // #
                    }
                    break;
                }
            }

            // Check for Jinja constructs
            if self.at_jinja_expression() {
                self.parse_jinja_expression(opening_hashes);
            } else if self.at_jinja_statement() {
                self.parse_jinja_statement(opening_hashes);
            } else if self.at_jinja_comment() {
                self.parse_jinja_comment(opening_hashes);
            } else {
                // Plain text content - collect tokens until we hit a Jinja construct or closing delimiter
                self.parse_prompt_text(opening_hashes);
            }
        }
    }

    /// Check if we're at the start of a Jinja expression: {{
    fn at_jinja_expression(&self) -> bool {
        self.at_raw(TokenKind::LBrace)
            && self.peek_impl(1, false).map(|t| t.kind) == Some(TokenKind::LBrace)
    }

    /// Check if we're at the start of a Jinja statement: {%
    fn at_jinja_statement(&self) -> bool {
        self.at_raw(TokenKind::LBrace)
            && self.peek_impl(1, false).map(|t| t.kind) == Some(TokenKind::Percent)
    }

    /// Check if we're at the start of a Jinja comment: {#
    fn at_jinja_comment(&self) -> bool {
        self.at_raw(TokenKind::LBrace)
            && self.peek_impl(1, false).map(|t| t.kind) == Some(TokenKind::Hash)
    }

    /// Parse a Jinja expression: {{ ... }}
    fn parse_jinja_expression(&mut self, opening_hashes: usize) {
        self.with_node(SyntaxKind::TEMPLATE_INTERPOLATION, |p| {
            p.bump_raw(); // {
            p.bump_raw(); // {

            // Collect tokens until we find }}
            let mut depth = 1;
            while !p.at_end_raw() && depth > 0 {
                if p.at_raw(TokenKind::Quote)
                    && p.count_consecutive_hashes_after_quote() == opening_hashes
                {
                    p.error_unexpected_token("Unclosed Jinja expression (expected }})".to_string());
                    return;
                }
                if p.at_raw(TokenKind::LBrace)
                    && p.peek_impl(1, false).map(|t| t.kind) == Some(TokenKind::LBrace)
                {
                    depth += 1;
                    p.bump_raw();
                    p.bump_raw();
                } else if p.at_raw(TokenKind::RBrace)
                    && p.peek_impl(1, false).map(|t| t.kind) == Some(TokenKind::RBrace)
                {
                    depth -= 1;
                    if depth == 0 {
                        p.bump_raw(); // }
                        p.bump_raw(); // }
                        break;
                    }
                    p.bump_raw();
                    p.bump_raw();
                } else {
                    p.bump_raw();
                }
            }

            if depth > 0 {
                p.error_unexpected_token("Unclosed Jinja expression (expected }})".to_string());
            }
        });
    }

    /// Parse a Jinja statement: {% ... %}
    fn parse_jinja_statement(&mut self, opening_hashes: usize) {
        self.with_node(SyntaxKind::TEMPLATE_CONTROL, |p| {
            p.bump_raw(); // {
            p.bump_raw(); // %

            // Collect tokens until we find %}
            while !p.at_end_raw() {
                if p.at_raw(TokenKind::Quote)
                    && p.count_consecutive_hashes_after_quote() == opening_hashes
                {
                    p.error_unexpected_token("Unclosed Jinja statement (expected %})".to_string());
                    return;
                }
                if p.at_raw(TokenKind::Percent)
                    && p.peek_impl(1, false).map(|t| t.kind) == Some(TokenKind::RBrace)
                {
                    p.bump_raw(); // %
                    p.bump_raw(); // }
                    break;
                }
                p.bump_raw();
            }
        });
    }

    /// Parse a Jinja comment: {# ... #}
    fn parse_jinja_comment(&mut self, opening_hashes: usize) {
        self.with_node(SyntaxKind::TEMPLATE_COMMENT, |p| {
            p.bump_raw(); // {
            p.bump_raw(); // #

            // Collect tokens until we find #}
            while !p.at_end_raw() {
                if p.at_raw(TokenKind::Quote)
                    && p.count_consecutive_hashes_after_quote() == opening_hashes
                {
                    p.error_unexpected_token("Unclosed Jinja comment (expected #})".to_string());
                    return;
                }
                if p.at_raw(TokenKind::Hash)
                    && p.peek_impl(1, false).map(|t| t.kind) == Some(TokenKind::RBrace)
                {
                    p.bump_raw(); // #
                    p.bump_raw(); // }
                    break;
                }
                p.bump_raw();
            }
        });
    }

    /// Parse plain text content between Jinja constructs
    ///
    /// Will consume trailing whitespace as well.
    fn parse_prompt_text(&mut self, opening_hashes: usize) {
        self.with_node(SyntaxKind::PROMPT_TEXT, |p| {
            // Collect tokens until we hit a Jinja construct or closing delimiter
            while !p.at_end_raw() {
                // Check for closing delimiter
                if p.at_raw(TokenKind::Quote) {
                    let closing_hashes = p.count_consecutive_hashes_after_quote();
                    if closing_hashes == opening_hashes {
                        break;
                    }
                }

                // Check for Jinja constructs
                if p.at_jinja_expression() || p.at_jinja_statement() || p.at_jinja_comment() {
                    while p.eat_basic_trivia() {} // make it part of the PROMPT_TEXT
                    break;
                }

                p.bump_raw();
            }
        });
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

    /// Parse an @attr: @alias("name") or @stream.done
    pub(crate) fn parse_at_attribute(&mut self) {
        // Eat leading trivia so whitespace stays outside the ATTRIBUTE node,
        // keeping the node's text_range tight around `@name(...)`.
        while self.eat_trivia() {}
        self.with_node(SyntaxKind::ATTRIBUTE, |p| {
            let at_span = p.current().map(|t| t.span);
            p.expect(TokenKind::At);

            if p.has_newline_ahead() {
                // if the attribute name is not on the same line as the @, that's an error
                if let Some(at_span) = at_span {
                    p.error("Attribute is missing a name".to_string(), at_span);
                }
                return;
            }

            // Attribute name (can be dotted like stream.done)
            if p.at(TokenKind::Word) {
                p.bump();
                // Handle dotted attribute names like @stream.done
                while p.at(TokenKind::Dot) {
                    p.bump(); // consume dot
                    if p.at(TokenKind::Word) {
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

    /// Parse an @@attr: @@dynamic or @@stream.done
    pub(crate) fn parse_atat_attribute(&mut self) {
        self.with_node(SyntaxKind::BLOCK_ATTRIBUTE, |p| {
            p.expect(TokenKind::AtAt);

            // Attribute name (can be dotted like @@stream.done)
            if p.at(TokenKind::Word) || p.at(TokenKind::Dynamic) || p.at(TokenKind::Throws) {
                p.bump();
                // Handle dotted attribute names like @@stream.done
                while p.at(TokenKind::Dot) {
                    p.bump(); // consume dot
                    if p.at(TokenKind::Word) || p.at(TokenKind::Dynamic) || p.at(TokenKind::Throws)
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
        // - Expression: @some_attr({{ this > 0 }})
        // - Unquoted string: @alias(my_alias) - one WORD token

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
            // Unquoted string: only permit one word
            self.with_node(SyntaxKind::UNQUOTED_STRING, |p| {
                p.bump();
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
        self.parse_type_with(true);
    }

    /// Parse a type expression. When `consume_union` is `false`, top-level
    /// `|` is left for the caller (used by pattern atoms, where `|` belongs
    /// to `UNION_PATTERN`).
    pub(crate) fn parse_type_with(&mut self, consume_union: bool) {
        self.with_node(SyntaxKind::TYPE_EXPR, |p| {
            p.parse_type_primary(consume_union);

            if p.pending_greaters > 0 {
                // Don't parse modifiers until we've used all
                // pending `>`
                return;
            }
            // Type modifiers
            loop {
                if p.at(TokenKind::LBracket) {
                    // Array type: string[]
                    p.bump(); // [
                    p.expect(TokenKind::RBracket); // ]
                } else if p.at(TokenKind::Question) {
                    // Optional type: string?
                    p.bump();
                } else if consume_union && p.at(TokenKind::Pipe) {
                    // Union type: string | int | "user" | "assistant"
                    p.bump();
                    p.parse_type_primary(consume_union);
                } else if p.at(TokenKind::At) {
                    // All attributes (both type and field) are consumed inside TYPE_EXPR.
                    // Disambiguation happens during lowering, which has the structural
                    // context to classify field vs type attributes.
                    if p.peek(1).is_none() {
                        break;
                    }
                    p.parse_at_attribute();
                } else {
                    break;
                }
            }
        });
    }

    fn parse_type_primary(&mut self, consume_union: bool) {
        // Check for string literal types: "user" | "assistant"
        if self.parse_any_string() {
            return;
        }

        // Negative numeric literal type: `-42`, `-3.14`. Recognised before
        // the unary-`-` falls through to the generic error path so literal
        // unions like `-1 | 0 | 1` and pattern atoms like `match { -42 => ... }`
        // parse uniformly. Floats still error to match the positive case.
        if self.at(TokenKind::Minus)
            && matches!(
                self.peek(1).map(|t| t.kind),
                Some(TokenKind::IntegerLiteral | TokenKind::FloatLiteral)
            )
        {
            let next_kind = self.peek(1).map(|t| t.kind);
            if next_kind == Some(TokenKind::FloatLiteral)
                && let Some(token) = self.peek(1)
            {
                let span = token.span;
                let text = token.text.clone();
                self.error(
                    format!("Float literal values are not supported: -{text}"),
                    span,
                );
            }
            self.bump(); // -
            self.bump(); // number
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

            // Consume dot-separated path segments (e.g., baml.http.Request)
            while self.at(TokenKind::Dot) {
                self.bump(); // dot
                if self.at(TokenKind::Word) {
                    self.bump(); // next segment
                } else {
                    self.error_unexpected_token("type name segment after '.'".to_string());
                    break;
                }
            }

            // Check for generic arguments: map<K, V>
            if self.at(TokenKind::Less) {
                self.type_args_depth += 1;
                self.with_node(SyntaxKind::TYPE_ARGS, |p| {
                    p.bump(); // <

                    p.parse_type();

                    while p.pending_greaters == 0 && p.eat(TokenKind::Comma) {
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
                    for _ in 0..self.pending_greaters {
                        self.events.push(Event::Token {
                            kind: SyntaxKind::GREATER,
                            text: ">".to_string(),
                        });
                    }
                    self.pending_greaters = 0;
                    self.pending_greater_span = None;
                }
            }
        } else if self.at(TokenKind::LParen) {
            // Could be:
            // 1. Parenthesized type: (int | string)
            // 2. Function type: (x: int, y: int) -> bool  OR  (int, int) -> bool
            //
            // We parse the contents as function type parameters (which can be either
            // `name: type` or just `type`), then check for `->` to determine which case.
            self.parse_paren_or_function_type(consume_union);
        } else {
            self.error_unexpected_token("type".to_string());
        }
    }

    /// Parse either a parenthesized type or a function type.
    ///
    /// Called when we see `(` in a type position. Could be:
    /// - Parenthesized type: `(int | string)` - single type, no arrow
    /// - Function type: `(x: int, y: int) -> bool` or `(int, int) -> bool`
    ///
    /// The key distinguisher is the presence of `->` after the closing `)`.
    fn parse_paren_or_function_type(&mut self, consume_union: bool) {
        // We'll parse the content first, then decide based on whether `->` follows.
        // For function types, we wrap in FUNCTION_TYPE; for parens, we just have the inner type.

        self.bump(); // (

        // Track whether any parameter had a name (which would make it invalid as parenthesized type)
        let mut had_named_param = false;

        // Parse parameters/types inside parens
        if !self.at(TokenKind::RParen) {
            had_named_param |= self.parse_function_type_param_inner();

            while self.eat(TokenKind::Comma) {
                if self.at(TokenKind::RParen) {
                    break;
                }
                had_named_param |= self.parse_function_type_param_inner();
            }
        }

        // Error recovery: if we're not at ')' yet, skip tokens until we find ')' or reach a recovery point
        if !self.at(TokenKind::RParen) {
            if let Some(token) = self.current() {
                let message = format!("Unexpected '{}' in type expression", token.text);
                self.error(message, token.span);
            }
            self.skip_to_balanced_paren();
        }

        self.expect(TokenKind::RParen);

        // Now check for `->` to determine if this is a function type
        if self.at(TokenKind::Arrow) {
            // This is a function type: wrap everything in FUNCTION_TYPE node
            // Note: The tokens are already emitted, we just need to parse the return type
            self.bump(); // ->
            // Forward `consume_union` so that pattern-atom callers (which
            // pass `false`) leave a trailing `|` to the surrounding
            // `UNION_PATTERN` instead of swallowing it into the return type.
            self.parse_type_with(consume_union); // return type
            if self.at(TokenKind::Throws) {
                self.with_node(SyntaxKind::THROWS_CLAUSE, |p| {
                    p.bump(); // throws
                    p.parse_type();
                });
            }
        // The caller's with_node(TYPE_EXPR) will wrap this appropriately
        } else {
            // Not a function type - should be a parenthesized type
            if had_named_param {
                // Error: named parameters require `->` to form a function type
                if let Some(token) = self.current() {
                    self.error(
                        "Named parameters in type expression require `->` to form a function type"
                            .to_string(),
                        token.span,
                    );
                }
            }
            // Note: We don't emit an error for multiple types without `->` because:
            // 1. Tuple types might be added in the future
            // 2. It allows for better error recovery
            // 3. The type checker will catch invalid types anyway
            // For single unnamed type, this is just a parenthesized type - that's fine
        }
    }

    /// Parse a single function type parameter: either `name: type` or just `type`.
    ///
    /// Returns `true` if this parameter had a name.
    fn parse_function_type_param_inner(&mut self) -> bool {
        // Check if this is `name: type` by looking ahead
        // If we see WORD followed by COLON (skipping trivia), it's a named param
        let is_named = (self.at(TokenKind::Word) || self.at(TokenKind::Client))
            && matches!(
                (self.peek(1).map(|t| t.kind), self.peek(2).map(|t| t.kind)),
                (Some(TokenKind::Colon), _) | (Some(TokenKind::Question), Some(TokenKind::Colon))
            );

        if is_named {
            // Named parameter: `name: type`
            self.with_node(SyntaxKind::FUNCTION_TYPE_PARAM, |p| {
                p.bump(); // name
                p.eat(TokenKind::Question); // optional parameter marker: `name?: T`
                p.expect(TokenKind::Colon);
                p.parse_type();
                if p.eat(TokenKind::Equals) {
                    let span = p.parse_default_expr();
                    p.error(
                        "default expressions are not allowed in function types".to_string(),
                        span,
                    );
                }
            });
            true
        } else {
            // Unnamed parameter: just `type`
            self.with_node(SyntaxKind::FUNCTION_TYPE_PARAM, |p| {
                p.parse_type();
            });
            false
        }
    }

    // ============ Enum Parsing ============

    /// Parse an enum declaration
    pub(crate) fn parse_enum(&mut self) {
        self.with_node(SyntaxKind::ENUM_DEF, |p| {
            while p.at(TokenKind::AtAt) {
                p.parse_atat_attribute();
            }

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
                    p.parse_atat_attribute();
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

            // Check if a type expression follows (which is invalid for enum variants)
            // We need to distinguish between:
            // - `A int | string` (type annotation - error)
            // - `A\n    B` (next variant - OK)
            // A type annotation is present if we see:
            // 1. A word followed by type modifiers (|, [, ?, <)
            // 2. String/integer/float literals (can't be variant names)
            // 3. Left paren (tuple type)
            let has_type_annotation = if p.at(TokenKind::Word) {
                // Peek ahead to see if there's a type modifier after the word
                // peek(1) skips trivia and gets the next non-trivia token
                p.peek(1)
                    .map(|t| {
                        matches!(
                            t.kind,
                            TokenKind::Pipe
                                | TokenKind::LBracket
                                | TokenKind::Question
                                | TokenKind::Less
                        )
                    })
                    .unwrap_or(false)
            } else {
                // String literals, integer literals, float literals, or left paren
                // These can't be variant names, so they must be type annotations
                p.at(TokenKind::Quote)
                    || p.at(TokenKind::Hash)
                    || p.at(TokenKind::IntegerLiteral)
                    || p.at(TokenKind::FloatLiteral)
                    || p.at(TokenKind::LParen)
            };

            if has_type_annotation {
                // Record the start position of the type expression
                // SAFETY: has_type_annotation is only true when we've confirmed a token exists
                // via p.at() checks or p.peek() calls, so current() is guaranteed to be Some.
                if let Some(start_token) = p.current() {
                    let start_span = start_token.span;

                    // Consume the entire type expression for error recovery
                    p.parse_type();

                    // Calculate the span covering the entire type expression
                    let end_span = p
                        .tokens
                        .get(p.current.saturating_sub(1))
                        .map(|t| t.span)
                        .unwrap_or(start_span);

                    let type_span = baml_base::Span::new(
                        start_span.file_id,
                        TextRange::new(start_span.range.start(), end_span.range.end()),
                    );

                    // Emit a helpful error message
                    p.error(
                        "Enum variants cannot have type annotations".to_string(),
                        type_span,
                    );
                }
            }

            // Optional field attributes (@alias, etc.)
            while p.at(TokenKind::At) && !p.at(TokenKind::AtAt) {
                p.parse_at_attribute();
            }
        });
    }

    // ============ Class Parsing ============

    /// Parse a class declaration
    pub(crate) fn parse_class(&mut self) {
        self.with_node(SyntaxKind::CLASS_DEF, |p| {
            while p.at(TokenKind::AtAt) {
                p.parse_atat_attribute();
            }

            // 'class' keyword
            p.expect(TokenKind::Class);

            // Class name
            if p.at(TokenKind::Word) {
                p.bump(); // name
            } else {
                p.error_unexpected_token("class name".to_string());
            }

            // Optional generic parameters: <T> or <K, V>
            if p.at(TokenKind::Less) {
                p.parse_generic_param_list();
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

                if p.at(TokenKind::AtAt)
                    && p.item_keyword_after_leading_block_attributes() == Some(TokenKind::Function)
                {
                    // Method definition with leading block attributes.
                    p.parse_function();
                } else if p.at(TokenKind::AtAt) {
                    // Block attribute: @@dynamic
                    p.parse_atat_attribute();
                } else if p.at(TokenKind::Function) {
                    // Method definition
                    p.parse_function();
                } else if p.at(TokenKind::Word) {
                    // Field declaration
                    p.parse_field();
                    if !p.eat(TokenKind::Comma) {
                        p.eat(TokenKind::Semicolon);
                    }
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

    /// Parse declaration-site generic parameter list: `<T>` or `<K, V>`.
    ///
    /// This is different from `GENERIC_ARGS` (call-site: `fetch<Response>(url)`).
    /// This produces `GENERIC_PARAM_LIST` containing `GENERIC_PARAM` children.
    fn parse_generic_param_list(&mut self) {
        self.with_node(SyntaxKind::GENERIC_PARAM_LIST, |p| {
            p.expect(TokenKind::Less); // <

            // Parse comma-separated type parameter names
            loop {
                if p.at(TokenKind::Greater) || p.at_end() {
                    break;
                }
                p.with_node(SyntaxKind::GENERIC_PARAM, |p| {
                    if p.at(TokenKind::Word) {
                        p.bump(); // type parameter name
                    } else {
                        p.error_unexpected_token("type parameter name".to_string());
                    }
                });
                if !p.eat(TokenKind::Comma) {
                    break;
                }
            }

            p.expect(TokenKind::Greater); // >
        });
    }

    fn parse_field(&mut self) {
        self.with_node(SyntaxKind::FIELD, |p| {
            // Field name - capture span and text before bumping
            let field_name_span = p.current().map(|t| t.span);
            let field_name_text = p.current().map(|t| t.text.clone());
            p.bump();

            let has_colon = p.eat(TokenKind::Colon);

            // Check if there's a newline before the next token
            // (newline means the type is on a different line - the field is incomplete)
            let newline_before_type = p.has_newline_ahead();

            // Field type - check if we're at a valid type start
            // If there was no colon, it must be on the same line
            let has_type = p.is_at_type_start() && (!newline_before_type || has_colon);
            if has_type {
                p.parse_type();
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
            while p.at(TokenKind::AtAt) {
                p.parse_atat_attribute();
            }

            // 'function' keyword
            p.expect(TokenKind::Function);

            // Function name
            if p.at(TokenKind::Word) {
                p.bump();
            } else {
                p.error_unexpected_token("function name".to_string());
                // Recovery: skip until we see '(', '{', or '->'
                while !p.at(TokenKind::LParen)
                    && !p.at(TokenKind::Less)
                    && !p.at(TokenKind::LBrace)
                    && !p.at(TokenKind::Arrow)
                    && !p.at_end()
                {
                    p.bump();
                }
            }

            // Optional generic parameters: <T> or <K, V>
            if p.at(TokenKind::Less) {
                p.parse_generic_param_list();
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
                p.parse_function_body(false);
                return;
            }

            // Parameters
            p.parse_parameter_list();

            // Return type
            let mut allow_llm_body = true;
            if p.eat(TokenKind::Arrow) {
                if p.at(TokenKind::LBrace) {
                    // The `{` belongs to the function body, not a return type.
                    // Keep recovery in expression-body mode so `client` text
                    // inside the broken body does not masquerade as an LLM
                    // directive.
                    allow_llm_body = false;
                }
                p.parse_type();
            } else {
                allow_llm_body = false;
                p.error_unexpected_token("return type (->)".to_string());
            }

            // Optional throws clause (BEP-007)
            if p.at(TokenKind::Throws) {
                p.with_node(SyntaxKind::THROWS_CLAUSE, |p| {
                    p.bump(); // throws
                    p.parse_type();
                });
            }

            // Body
            if p.at(TokenKind::LBrace) {
                p.parse_function_body(allow_llm_body);
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

            // Parameter name (`client` lexes as KW_CLIENT, not Word)
            if p.at(TokenKind::Word) || p.at(TokenKind::Client) {
                p.bump();
            } else {
                p.error_unexpected_token("parameter name".to_string());
            }

            // Type annotation - colon is optional per BEP-019
            // 'self' parameter does not have a type annotation
            if is_self {
                // No type annotation for self, but consume an optional default
                // for recovery so lowering can emit a context-specific diagnostic.
                if p.eat(TokenKind::Equals) {
                    p.parse_default_expr();
                }
                return;
            }

            let has_colon = p.eat(TokenKind::Colon);

            // Check if there's a newline before the next token.
            // Consistent with class field parsing: if no colon, type must be on the same line.
            let newline_before_type = p.has_newline_ahead();
            let has_type = p.is_at_type_start() && (!newline_before_type || has_colon);
            if has_type {
                p.parse_type();
            } else {
                p.error_unexpected_token("type annotation".to_string());
            }

            if p.eat(TokenKind::Equals) {
                p.parse_default_expr();
            }
        });
    }

    fn parse_function_body(&mut self, allow_llm_body: bool) {
        // Scan tokens to determine function type before parsing (single pass)
        if allow_llm_body && self.looks_like_llm_function_body() {
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
            let new_i = self.skip_comment_at(i);
            if new_i != i {
                i = new_i;
                continue;
            }

            let new_i = self.skip_header_comment_at(i);
            if new_i != i {
                i = new_i;
                continue;
            }

            let token = &self.tokens[i];
            if self.is_basic_trivia(token.kind) {
                i += 1;
                continue;
            }

            match token.kind {
                TokenKind::LBrace => brace_depth += 1,
                TokenKind::RBrace if brace_depth == 1 => break,
                TokenKind::RBrace => brace_depth -= 1,
                TokenKind::Word if brace_depth == 1 => {
                    let text = &token.text;
                    if text == "client" || text == "prompt" {
                        return true;
                    }
                }
                // `client` as KW_CLIENT: LLM directive is `client Model`, not `client.method(...)`.
                TokenKind::Client if brace_depth == 1 => {
                    let j = self.skip_trivia_and_comments_from(i + 1);
                    let next = self.tokens.get(j).map(|t| t.kind);
                    if next != Some(TokenKind::Dot) {
                        return true;
                    }
                }
                // Check for expression function keywords
                TokenKind::Let
                | TokenKind::Return
                | TokenKind::If
                | TokenKind::While
                | TokenKind::For
                | TokenKind::Throw
                | TokenKind::Catch
                | TokenKind::CatchAll
                    if brace_depth == 1 =>
                {
                    return false;
                }
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
                // Error recovery: if we see a top-level keyword (except Client and TypeBuilder)
                // assume we missed a closing brace
                if p.at_top_level_keyword()
                    && !p.at(TokenKind::Client)
                    && !p.at(TokenKind::TypeBuilder)
                {
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
                } else if p.at(TokenKind::TypeBuilder) {
                    // Parse type_builder block - HIR will emit proper error for non-test context
                    p.parse_type_builder_block();
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

            // Optional colon
            p.eat(TokenKind::Colon);

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

            // Optional colon
            p.eat(TokenKind::Colon);

            // Prompt value (usually a raw string)
            if !p.parse_any_string() {
                p.error_unexpected_token("prompt string".to_string());
            }
        });
    }

    /// Parse a lambda expression:
    ///   `[<T, U>] (params) -> [RetType] [throws E] { body }`
    fn parse_lambda_expr(&mut self) {
        self.with_node(SyntaxKind::LAMBDA_EXPR, |p| {
            // Optional generic parameters: <T> or <K, V>
            if p.at(TokenKind::Less) {
                p.parse_generic_param_list();
            }

            // Parameter list: (x: int, y: string) or (x, y) or ()
            // Lambda params have optional type annotations (unlike function params)
            p.parse_lambda_parameter_list();

            // Arrow is required. Accept `->` (canonical) or `=>` (formatter
            // will normalize to `->`, matching the optional-colon pattern for
            // function parameters).
            if !p.eat(TokenKind::Arrow) && !p.eat(TokenKind::FatArrow) {
                p.error_unexpected_token("'->' after lambda parameters".to_string());
            }

            // Optional return type: anything before `throws` or `{`
            if !p.at(TokenKind::LBrace) && !p.at(TokenKind::Throws) {
                p.parse_type();
            }

            // Optional throws clause
            if p.at(TokenKind::Throws) {
                p.with_node(SyntaxKind::THROWS_CLAUSE, |p| {
                    p.bump(); // throws
                    p.parse_type();
                });
            }

            // Body: block expression (required)
            if p.at(TokenKind::LBrace) {
                p.parse_block_expr();
            } else {
                p.error_unexpected_token("lambda body '{'".to_string());
            }
        });
    }

    /// Parse a lambda parameter list where type annotations are optional.
    fn parse_lambda_parameter_list(&mut self) {
        self.with_node(SyntaxKind::PARAMETER_LIST, |p| {
            p.expect(TokenKind::LParen);

            if !p.at(TokenKind::RParen) {
                p.parse_lambda_parameter();

                while p.eat(TokenKind::Comma) {
                    if p.at(TokenKind::RParen) {
                        break; // Trailing comma
                    }
                    p.parse_lambda_parameter();
                }
            }

            p.expect(TokenKind::RParen);
        });
    }

    /// Parse a single lambda parameter with an optional type annotation.
    fn parse_lambda_parameter(&mut self) {
        self.with_node(SyntaxKind::PARAMETER, |p| {
            // Parameter name
            if p.at(TokenKind::Word) || p.at(TokenKind::Client) {
                p.bump();
            } else {
                p.error_unexpected_token("parameter name".to_string());
            }

            // Optional type annotation: "name: type"
            if p.eat(TokenKind::Colon) {
                p.parse_type();
            }

            if p.eat(TokenKind::Equals) {
                p.parse_default_expr();
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
                if p.at_top_level_keyword_except_client() {
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
        } else if self.at(TokenKind::Throw) {
            self.parse_throw_stmt();
        } else if self.at(TokenKind::Test) && self.looks_like_test_expr_body() {
            if self.testset_body_depth > 0 {
                self.parse_test_expr();
            } else {
                let span = self.current().map(|t| t.span).unwrap_or_else(|| {
                    baml_base::Span::new(baml_base::FileId::new(0), TextRange::default())
                });
                self.error(
                    "test blocks are only allowed at the top level or inside a testset".to_string(),
                    span,
                );
                self.parse_test_expr(); // still parse to recover
            }
        } else if self.at(TokenKind::TestSet) {
            if self.testset_body_depth > 0 {
                self.parse_testset();
            } else {
                let span = self.current().map(|t| t.span).unwrap_or_else(|| {
                    baml_base::Span::new(baml_base::FileId::new(0), TextRange::default())
                });
                self.error(
                    "testset blocks are only allowed at the top level or inside a testset"
                        .to_string(),
                    span,
                );
                self.parse_testset(); // still parse to recover
            }
        } else {
            // Expression statement
            self.parse_expr_stmt();
        }
    }

    fn parse_let_stmt(&mut self) {
        self.with_node(SyntaxKind::LET_STMT, |p| {
            // A let statement's pattern must start with `let`. parse_pattern
            // itself is permissive (it parses any pattern shape, including
            // ones with no binding), so we enforce the `let` keyword here
            // before delegating. The keyword is consumed inside parse_pattern.
            if !p.at(TokenKind::Let) {
                p.error_unexpected_token("'let'".to_string());
            }
            if p.peek(1).map(|t| t.kind) == Some(TokenKind::LBracket) {
                p.bump(); // statement `let`
                p.parse_pattern();
            } else {
                p.parse_pattern();
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
            // Same invariant as parse_let_stmt: pattern must start with `let`.
            if !p.at(TokenKind::Let) {
                p.error_unexpected_token("'let'".to_string());
            }
            if p.peek(1).map(|t| t.kind) == Some(TokenKind::LBracket) {
                p.bump(); // statement `let`
                p.parse_pattern();
            } else {
                p.parse_pattern();
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

            // Optional return value — bare `return;` is valid (e.g. in void functions).
            if !p.at(TokenKind::Semicolon) && !p.at(TokenKind::RBrace) && !p.at_end() {
                p.parse_expr();
            }

            // Consume trailing semicolon
            p.eat(TokenKind::Semicolon);
        });
    }

    fn parse_throw_stmt(&mut self) {
        self.with_node(SyntaxKind::THROW_STMT, |p| {
            // Parse as a full expression so `throw x catch (...)` is handled
            // as one throw statement with catch attached to the throw.
            p.parse_expr();
            p.eat(TokenKind::Semicolon);
        });
    }

    fn parse_throw_expr(&mut self) {
        self.with_node(SyntaxKind::THROW_EXPR, |p| {
            p.expect(TokenKind::Throw);

            // Throw requires a payload expression.
            if p.at(TokenKind::Semicolon) || p.at(TokenKind::Comma) || p.at(TokenKind::RBrace) {
                p.error_unexpected_token("expression after 'throw'".to_string());
                return;
            }

            if p.at_end() {
                p.error_unexpected_token("expression after 'throw'".to_string());
                return;
            }

            p.parse_expr_bp_no_catch(0);
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
                // Optional type annotation: match (expr : Type)
                if p.eat(TokenKind::Colon) {
                    p.parse_type();
                }
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
                        if p.at_top_level_keyword_except_client() {
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
            p.parse_pattern();

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

    /// Parse a match guard.
    ///
    /// Grammar: guard := 'if' expr
    fn parse_match_guard(&mut self) {
        self.with_node(SyntaxKind::MATCH_GUARD, |p| {
            p.expect(TokenKind::If);
            p.parse_expr();
        });
    }

    // ============ Patterns ============
    //
    // Grammar:
    //   PATTERN     := CHAIN
    //   CHAIN       := UNION (':' UNION)*
    //   UNION       := ATOM ('|' ATOM)*
    //   ATOM        := BINDING_PATTERN
    //                | DESTRUCTURE_PATTERN
    //                | ARRAY_PATTERN
    //                | TYPE_PATTERN
    //                | PAREN_PATTERN
    //
    // `:` splits before `|`, so `let x: int | string` is `let x : (int | string)`.

    /// Parse a pattern. Always wraps the result in a `PATTERN` node.
    /// Patterns are `union ('|' union)*` of atoms. The `:` type-ascription
    /// is a grammar property of the `BINDING_PATTERN` and `ARRAY_PATTERN`
    /// atoms (handled inside `parse_let_pattern` / `parse_array_pattern`),
    /// not a separate combinator.
    fn parse_pattern(&mut self) {
        self.with_node(SyntaxKind::PATTERN, |p| {
            p.parse_pattern_union();
        });
    }

    /// Parse a `|`-separated union. Wraps in `UNION_PATTERN` only if at least
    /// one `|` is present.
    fn parse_pattern_union(&mut self) {
        let union_start = self.events.len();
        self.parse_pattern_atom();
        if !self.at(TokenKind::Pipe) {
            return;
        }
        while self.at(TokenKind::Pipe) {
            self.bump(); // |
            self.parse_pattern_atom();
        }
        self.wrap_events_in_node(union_start, SyntaxKind::UNION_PATTERN);
        self.finish_node();
    }

    /// Parse a single atomic pattern.
    fn parse_pattern_atom(&mut self) {
        if self.at(TokenKind::LParen) {
            // `(` may start either a parenthesized pattern OR a parenthesized
            // type expression. Type expression iff the matching `)` is
            // followed by `->` (function type) or `[` / `?` (array / optional
            // suffix on a paren'd type, e.g. `(int | string)[]`).
            //
            // Function-type paren keeps `|` for the surrounding `UNION_PATTERN`
            // (so `(int) -> int | string` parses as `Or([fn, string])`).
            // Paren-type-suffix paren consumes `|` because the whole
            // expression is unambiguously one type — `(int | string)[] | float`
            // parses as the union type `(int | string)[] | float`.
            if self.looks_like_function_type_paren() {
                self.with_node(SyntaxKind::TYPE_PATTERN, |p| {
                    p.parse_type_with(/* consume_union = */ false);
                });
            } else if self.looks_like_paren_type_suffix() {
                self.with_node(SyntaxKind::TYPE_PATTERN, |p| {
                    p.parse_type_with(/* consume_union = */ true);
                });
            } else {
                self.with_node(SyntaxKind::PAREN_PATTERN, |p| {
                    p.bump(); // (
                    p.parse_pattern();
                    p.expect(TokenKind::RParen);
                });
            }
            return;
        }

        if self.at(TokenKind::Let) {
            self.parse_let_pattern();
            return;
        }

        if self.at(TokenKind::LBracket) {
            self.parse_array_pattern();
            return;
        }

        // Bare `_` is a wildcard. Recognise before the destructure check so
        // `_ { ... }` doesn't parse as a destructure on a class literally
        // named `_`.
        if self.at_wildcard_word() {
            self.with_node(SyntaxKind::WILDCARD_PATTERN, |p| {
                p.bump();
            });
            return;
        }

        // A bare WORD may start a destructure (`Class { ... }`,
        // `Class<int> { ... }`) or a type/path pattern. Look ahead through
        // dotted segments and optional trailing generic args for a `{`.
        if self.at(TokenKind::Word) && self.looks_like_destructure_pattern() {
            self.parse_destructure_pattern(false);
            return;
        }

        // Anything else is a bare type-expression pattern: literals (`1`,
        // `"user"`, `true`), paths (`Status.Active`), generics (`Foo<T>`),
        // arrays (`int[]`), etc. `consume_union = false` leaves top-level `|`
        // for the surrounding `UNION_PATTERN`.
        if !self.is_at_pattern_atom_start() {
            self.error_unexpected_token("pattern".to_string());
            if !self.at_end() {
                self.bump();
            }
            return;
        }

        self.with_node(SyntaxKind::TYPE_PATTERN, |p| {
            p.parse_type_with(/* consume_union = */ false);
        });
    }

    /// Called when at `(`. Returns `true` if the matching `)` is followed by
    /// `->`, indicating this opens a function-type pattern atom rather than a
    /// parenthesized pattern.
    ///
    /// Walks forward via `peek()` with a stack of expected closers, so
    /// `(MyScores { f: int }) -> Result` and `(int[]) -> int` are recognised
    /// correctly. On any mismatch (unbalanced or out-of-order closers) we
    /// bail out and let the caller treat it as a paren pattern; the real
    /// error will surface during the actual parse.
    /// True if `(...)` is followed by a type-expression suffix (`[` or `?`),
    /// indicating a parenthesized type like `(int | string)[]` rather than a
    /// parenthesized pattern.
    fn looks_like_paren_type_suffix(&self) -> bool {
        debug_assert!(self.at(TokenKind::LParen));
        let mut stack: Vec<TokenKind> = vec![TokenKind::RParen];
        let mut i: usize = 1;
        loop {
            let Some(tok) = self.peek(i) else {
                return false;
            };
            match tok.kind {
                TokenKind::LParen => stack.push(TokenKind::RParen),
                TokenKind::LBracket => stack.push(TokenKind::RBracket),
                TokenKind::LBrace => stack.push(TokenKind::RBrace),
                close @ (TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace) => {
                    let Some(expected) = stack.pop() else {
                        return false;
                    };
                    if expected != close {
                        return false;
                    }
                    if stack.is_empty() {
                        return matches!(
                            self.peek(i + 1).map(|t| t.kind),
                            Some(TokenKind::LBracket | TokenKind::Question)
                        );
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }

    fn looks_like_function_type_paren(&self) -> bool {
        debug_assert!(self.at(TokenKind::LParen));
        let mut stack: Vec<TokenKind> = vec![TokenKind::RParen];
        let mut i: usize = 1;
        loop {
            let Some(tok) = self.peek(i) else {
                return false;
            };
            match tok.kind {
                TokenKind::LParen => stack.push(TokenKind::RParen),
                TokenKind::LBracket => stack.push(TokenKind::RBracket),
                TokenKind::LBrace => stack.push(TokenKind::RBrace),
                close @ (TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace) => {
                    let Some(expected) = stack.pop() else {
                        return false;
                    };
                    if expected != close {
                        return false;
                    }
                    if stack.is_empty() {
                        // We just closed the outermost `(`. Function type iff
                        // the next non-trivia token is `->`.
                        return self.peek(i + 1).map(|t| t.kind) == Some(TokenKind::Arrow);
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }

    /// True if the current token can start a pattern atom (used for
    /// error-recovery sniffing).
    fn is_at_pattern_atom_start(&self) -> bool {
        matches!(
            self.current().map(|t| t.kind),
            Some(
                TokenKind::Word
                    | TokenKind::IntegerLiteral
                    | TokenKind::FloatLiteral
                    | TokenKind::Quote
                    | TokenKind::Hash
                    | TokenKind::Minus
                    | TokenKind::LParen
                    | TokenKind::LBracket
                    | TokenKind::Let
            )
        )
    }

    /// Look ahead from the current position (which should be at `WORD`) past
    /// any dotted path segments. Returns true if the next non-trivia token is
    /// `{`, signalling a class destructure pattern.
    fn looks_like_destructure_pattern(&self) -> bool {
        // We're at a WORD. Walk forward over `(WORD)('.' WORD)*`, optional
        // trailing generic args, and check for `{` at the end.
        let Some(mut idx) = self.current_non_trivia_index() else {
            return false;
        };
        // Skip first WORD.
        idx = self.skip_trivia_and_comments_from(idx + 1);
        loop {
            if idx >= self.tokens.len() {
                return false;
            }
            match self.tokens[idx].kind {
                TokenKind::Dot => {
                    let next = self.skip_trivia_and_comments_from(idx + 1);
                    if next < self.tokens.len() && self.tokens[next].kind == TokenKind::Word {
                        idx = self.skip_trivia_and_comments_from(next + 1);
                    } else {
                        return false;
                    }
                }
                TokenKind::LBrace => return true,
                TokenKind::Less if self.looks_like_generic_args_from(idx) => {
                    let Some(close) = self.find_matching_generic_args_close_from(idx) else {
                        return false;
                    };
                    idx = self.skip_trivia_and_comments_from(close + 1);
                    if idx < self.tokens.len() && self.tokens[idx].kind == TokenKind::LBrace {
                        return true;
                    }
                    return false;
                }
                _ => return false,
            }
        }
    }

    fn looks_like_generic_args_from(&self, start: usize) -> bool {
        if self.tokens.get(start).map(|t| t.kind) != Some(TokenKind::Less) {
            return false;
        }
        let Some(close) = self.find_matching_generic_args_close_from(start) else {
            return false;
        };
        let follow = self.skip_trivia_and_comments_from(close + 1);
        Self::is_generic_args_follow(self.tokens.get(follow).map(|t| t.kind))
    }

    fn find_matching_generic_args_close_from(&self, start: usize) -> Option<usize> {
        let mut depth: i32 = 1;
        let mut i = self.skip_trivia_and_comments_from(start + 1);
        while i < self.tokens.len() {
            match self.tokens[i].kind {
                TokenKind::Less => depth += 1,
                TokenKind::Greater => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                TokenKind::GreaterGreater => {
                    if depth < 2 {
                        return None;
                    }
                    depth -= 2;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                TokenKind::Word
                | TokenKind::Dot
                | TokenKind::Comma
                | TokenKind::LBracket
                | TokenKind::RBracket
                | TokenKind::Question
                | TokenKind::Pipe
                | TokenKind::IntegerLiteral
                | TokenKind::FloatLiteral
                | TokenKind::Minus
                | TokenKind::Quote
                | TokenKind::Hash
                | TokenKind::LParen
                | TokenKind::RParen => {}
                _ => return None,
            }
            i = self.skip_trivia_and_comments_from(i + 1);
        }
        None
    }

    /// Parse a `let`-prefixed pattern. Either:
    /// - `let _`           — `WILDCARD_PATTERN`
    /// - `let WORD`        — simple `BINDING_PATTERN`
    /// - `let PATH { fields }` — `DESTRUCTURE_PATTERN` with a `let` prefix
    fn parse_let_pattern(&mut self) {
        debug_assert!(self.at(TokenKind::Let));
        let start = self.events.len();
        self.bump(); // let

        if !self.at(TokenKind::Word) {
            self.error_unexpected_token("identifier after 'let'".to_string());
            self.wrap_events_in_node(start, SyntaxKind::BINDING_PATTERN);
            self.finish_node();
            return;
        }

        // `let _` is a wildcard, not a binding to a name called `_`.
        if self.at_wildcard_word() {
            self.bump(); // _
            self.wrap_events_in_node(start, SyntaxKind::WILDCARD_PATTERN);
            self.finish_node();
            return;
        }

        // Decide between simple binding (`let x`) and destructure
        // (`let Class { ... }`) by peeking past dotted segments for `{`.
        if self.looks_like_destructure_pattern() {
            self.parse_path();
            if self.at(TokenKind::Less) && self.looks_like_generic_args() {
                self.parse_generic_args();
            }
            self.parse_destructure_field_list();
            self.wrap_events_in_node(start, SyntaxKind::DESTRUCTURE_PATTERN);
            self.finish_node();
            return;
        }

        self.bump(); // WORD
        // Optional `: <pattern>` sub-pattern. Folded directly into the
        // BINDING_PATTERN node. The right side is any pattern — type
        // ascription (`let x: int`), aliasing chain (`let x: let y`),
        // structural destructure (`let x: [a, b]`, `let x: Class { f }`),
        // etc. `let x: int | string` consumes `|` as type-union syntax
        // (use explicit parens for `(let x: int) | (let y: string)`).
        if self.at(TokenKind::Colon) {
            self.bump(); // :
            self.parse_pattern();
        }
        self.wrap_events_in_node(start, SyntaxKind::BINDING_PATTERN);
        self.finish_node();
    }

    /// True if the current token is a `WORD` whose text is exactly `_`.
    /// Used to recognise the wildcard atom (`_` or `let _`).
    fn at_wildcard_word(&self) -> bool {
        self.at(TokenKind::Word) && self.current().map(|t| t.text.as_str()) == Some("_")
    }

    /// Parse a class destructure pattern: `PATH '{' field_list '}'`.
    /// `has_let` is informational; if the caller already consumed `let`, this
    /// function still works because the `let` token was emitted before the
    /// wrapper begins (callers use `wrap_events_in_node` themselves in that
    /// case — see `parse_let_pattern`). For pattern atoms with no `let`, this
    /// emits a self-contained `DESTRUCTURE_PATTERN`.
    fn parse_destructure_pattern(&mut self, _has_let: bool) {
        self.with_node(SyntaxKind::DESTRUCTURE_PATTERN, |p| {
            p.parse_path();
            if p.at(TokenKind::Less) && p.looks_like_generic_args() {
                p.parse_generic_args();
            }
            p.parse_destructure_field_list();
        });
    }

    /// Parse a dotted path: `WORD ('.' WORD)*`. Tokens are emitted into the
    /// current node — no wrapper node is added.
    fn parse_path(&mut self) {
        if !self.at(TokenKind::Word) {
            self.error_unexpected_token("identifier".to_string());
            return;
        }
        self.bump(); // first WORD
        while self.at(TokenKind::Dot) && self.peek(1).map(|t| t.kind) == Some(TokenKind::Word) {
            self.bump(); // .
            self.bump(); // WORD
        }
    }

    /// Parse `'{' field_pattern (',' field_pattern)* ','? '}'`.
    fn parse_destructure_field_list(&mut self) {
        if !self.expect(TokenKind::LBrace) {
            return;
        }

        while !self.at(TokenKind::RBrace) && !self.at_end() {
            if self.at_top_level_keyword_except_client() {
                break;
            }
            self.parse_field_pattern();

            if self.at(TokenKind::RBrace) {
                break;
            }
            if !self.eat(TokenKind::Comma) {
                self.error_unexpected_token("',' or '}' after field pattern".to_string());
                // Avoid infinite loop on garbage.
                if !self.at(TokenKind::Word) && !self.at(TokenKind::RBrace) && !self.at_end() {
                    self.bump();
                }
            }
        }

        self.expect(TokenKind::RBrace);
    }

    /// Parse a single field pattern: `WORD` (shorthand) or `WORD ':' PATTERN`.
    fn parse_field_pattern(&mut self) {
        self.with_node(SyntaxKind::FIELD_PATTERN, |p| {
            if !p.at(TokenKind::Word) {
                p.error_unexpected_token("field name".to_string());
                if !p.at_end() && !p.at(TokenKind::Comma) && !p.at(TokenKind::RBrace) {
                    p.bump();
                }
                return;
            }
            p.bump(); // field name

            if p.eat(TokenKind::Colon) {
                p.parse_pattern();
            }
        });
    }

    /// Parse an array destructure pattern:
    /// `'[' (PATTERN | '..' PATTERN?) (',' ...)* ','? ']'`.
    ///
    /// Array slots are normal pattern positions. A binding must therefore be
    /// written with `let`, e.g. `[let first, ..let rest]`.
    fn parse_array_pattern(&mut self) {
        self.with_node(SyntaxKind::ARRAY_PATTERN, |p| {
            if !p.expect(TokenKind::LBracket) {
                return;
            }

            let mut seen_rest = false;
            while !p.at(TokenKind::RBracket) && !p.at_end() {
                if p.at(TokenKind::DotDot) {
                    if seen_rest {
                        p.error_unexpected_token(
                            "only one '..' rest pattern is allowed in an array pattern".to_string(),
                        );
                    }
                    seen_rest = true;
                }
                p.parse_array_pattern_element();

                if p.at(TokenKind::RBracket) {
                    break;
                }
                if !p.eat(TokenKind::Comma) {
                    p.error_unexpected_token("',' or ']' after array pattern element".to_string());
                    if !p.at(TokenKind::RBracket) && !p.at_end() {
                        p.bump();
                    }
                }
            }

            p.expect(TokenKind::RBracket);

            // Optional `: T` type ascription — folded into the
            // ARRAY_PATTERN node. Consumes `|` as type-union syntax.
            if p.at(TokenKind::Colon) {
                p.bump(); // :
                p.parse_type_with(/* consume_union = */ true);
            }
        });
    }

    fn parse_array_pattern_element(&mut self) {
        self.with_node(SyntaxKind::ARRAY_PATTERN_ELEMENT, |p| {
            if p.eat(TokenKind::DotDot) {
                if !p.at(TokenKind::Comma) && !p.at(TokenKind::RBracket) && !p.at_end() {
                    p.parse_pattern();
                }
                return;
            }

            p.parse_pattern();
        });
    }

    fn at_catch_clause_start(&self) -> bool {
        self.at(TokenKind::Catch) || self.at(TokenKind::CatchAll)
    }

    fn parse_catch_expr(&mut self, expr_start: usize) {
        let lhs_start = self.find_previous_expr_start_after(expr_start);
        self.wrap_events_in_node(lhs_start, SyntaxKind::CATCH_EXPR);
        while self.at_catch_clause_start() {
            self.parse_catch_clause();
        }
        self.finish_node();
    }

    fn parse_catch_clause(&mut self) {
        self.with_node(SyntaxKind::CATCH_CLAUSE, |p| {
            if p.at_catch_clause_start() {
                p.bump();
            } else {
                p.error_unexpected_token("catch clause keyword".to_string());
                return;
            }

            if !p.expect(TokenKind::LParen) {
                return;
            }
            // The catch binding is always a bare identifier (like a function
            // parameter), not a full pattern.
            p.with_node(SyntaxKind::CATCH_BINDING, |p| {
                if p.at(TokenKind::Word) {
                    p.bump();
                } else {
                    p.error_unexpected_token("catch binding identifier".to_string());
                }
            });
            // Optional second binding: catch (e, stack_trace)
            if p.at(TokenKind::Comma) {
                p.bump(); // consume ','
                p.with_node(SyntaxKind::CATCH_STACK_TRACE_BINDING, |p| {
                    p.expect(TokenKind::Word);
                });
            }
            p.expect(TokenKind::RParen);

            if !p.at(TokenKind::LBrace) {
                p.error_unexpected_token("catch clause body".to_string());
                return;
            }

            p.bump(); // {

            if p.at(TokenKind::RBrace) {
                p.error_unexpected_token("at least one catch arm".to_string());
            } else {
                p.parse_catch_arm();
                while !p.at(TokenKind::RBrace) && !p.at_end() {
                    if p.at_top_level_keyword_except_client() {
                        break;
                    }
                    p.parse_catch_arm();
                }
            }

            p.expect(TokenKind::RBrace);
        });
    }

    fn parse_catch_arm(&mut self) {
        self.with_node(SyntaxKind::CATCH_ARM, |p| {
            p.parse_pattern();

            if p.at(TokenKind::FatArrow) {
                p.bump(); // =>
            } else {
                p.error_unexpected_token("'=>' after catch pattern".to_string());
                p.recover_catch_arm();
                return;
            }

            if p.at(TokenKind::LBrace) {
                p.parse_block_expr();
            } else {
                p.parse_expr();
            }

            p.eat(TokenKind::Comma);
        });
    }

    fn recover_catch_arm(&mut self) {
        while !self.at_end() {
            if self.at(TokenKind::RBrace) {
                break;
            }
            if self.eat(TokenKind::Comma) {
                break;
            }
            if self.looks_like_catch_arm_start() {
                break;
            }
            self.bump();
        }
    }

    fn current_non_trivia_index(&self) -> Option<usize> {
        let mut i = self.current;

        while i < self.tokens.len() {
            let new_i = self.skip_comment_at(i);
            if new_i != i {
                i = new_i;
                continue;
            }

            let kind = self.tokens[i].kind;
            if self.is_basic_trivia(kind) {
                i += 1;
                continue;
            }

            return Some(i);
        }

        None
    }

    fn looks_like_catch_arm_start(&self) -> bool {
        let mut i = self.current_non_trivia_index().unwrap_or(self.current);
        let mut scanned = 0usize;

        while i < self.tokens.len() && scanned < 64 {
            let new_i = self.skip_comment_at(i);
            if new_i != i {
                i = new_i;
                continue;
            }

            let kind = self.tokens[i].kind;
            if self.is_basic_trivia(kind) {
                i += 1;
                continue;
            }

            if matches!(
                kind,
                TokenKind::RBrace | TokenKind::Comma | TokenKind::Semicolon
            ) {
                return false;
            }

            if kind == TokenKind::FatArrow {
                return true;
            }

            i += 1;
            scanned += 1;
        }

        false
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
                    // Bare WORD inside parens may be a C-style header starting
                    // with an expression (`for (i = 0; cond; update)`). It
                    // can NOT be a let-less iterator form like `for (i in
                    // expr)` — bindings always require `let`. Defer to the
                    // C-style path.
                    p.parse_c_style_for_body();
                } else if p.at(TokenKind::Semicolon) {
                    // C-style with empty initializer: for (; cond; update)
                    p.parse_c_style_for_body();
                } else {
                    p.error_unexpected_token("'let' or ';'".to_string());
                }

                p.expect(TokenKind::RParen);
            } else {
                // Non-parenthesized form: `for let <pattern> in <expr> { }`.
                // The `let` is required — bindings always require it.
                p.parse_for_in_pattern();
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

    /// Check if this looks like a for-in loop. We're at `let`. Scan forward
    /// past the (possibly complex) pattern that follows: bindings, paths,
    /// destructures (`{ ... }`), parenthesised groups, type annotations
    /// after `:`. Whichever of `in` / `=` / `;` we hit at the top level
    /// (no open brackets/parens/braces) decides the form: `in` →
    /// iterator-style, `=` or `;` → C-style.
    ///
    /// Uses a stack of expected closers so out-of-order delimiters (e.g.
    /// `( [ ) ]`) bail out rather than mis-classify.
    fn looks_like_for_in_loop(&self) -> bool {
        debug_assert!(self.at(TokenKind::Let));
        let mut stack: Vec<TokenKind> = Vec::new();
        let mut i: usize = 1; // start after `let`
        loop {
            let Some(tok) = self.peek(i) else {
                return false;
            };
            match tok.kind {
                TokenKind::LParen => stack.push(TokenKind::RParen),
                TokenKind::LBracket => stack.push(TokenKind::RBracket),
                TokenKind::LBrace => stack.push(TokenKind::RBrace),
                close @ (TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace) => {
                    let Some(expected) = stack.pop() else {
                        // Unbalanced closer at top level — we've walked off
                        // the end of the for-header. Whatever this is,
                        // it's not an iterator form.
                        return false;
                    };
                    if expected != close {
                        return false;
                    }
                }
                TokenKind::In if stack.is_empty() => return true,
                TokenKind::Equals | TokenKind::Semicolon if stack.is_empty() => return false,
                _ => {}
            }
            i += 1;
        }
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
            // For-in pattern shares the let-statement shape: must start with
            // `let`. No initializer.
            if !p.at(TokenKind::Let) {
                p.error_unexpected_token("'let'".to_string());
            }
            if p.peek(1).map(|t| t.kind) == Some(TokenKind::LBracket) {
                p.bump(); // statement `let`
                p.parse_pattern();
            } else {
                p.parse_pattern();
            }
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

    /// Parse an expression where postfix `catch` must not bind to the payload.
    ///
    /// Used by prefix expression forms (e.g. `throw`) whose payload should not
    /// consume a trailing `catch` clause.  For example, `throw x catch (...)`
    /// must parse as `(throw x) catch (...)`, not `throw (x catch (...))`.
    fn parse_expr_bp_no_catch(&mut self, min_bp: u8) {
        self.suppress_catch_depth += 1;
        self.parse_expr_bp(min_bp);
        self.suppress_catch_depth -= 1;
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
            if self.suppress_catch_depth == 0 && self.at_catch_clause_start() {
                self.parse_catch_expr(expr_start);
                continue;
            } else if op == TokenKind::Less && self.looks_like_generic_args() {
                // Parse as generic arguments: foo<T>
                let lhs_start = self.find_previous_expr_start_after(expr_start);
                self.wrap_events_in_node(lhs_start, SyntaxKind::PATH_EXPR);
                self.parse_generic_args();
                self.finish_node();
                // Continue to potentially parse function call
                continue;
            } else if op == TokenKind::Question
                && self.peek(1).map(|t| t.kind) == Some(TokenKind::Question)
            {
                // Null coalescing operator `??`
                // Lexed as two Question tokens to avoid ambiguity with `int??` (double optional).
                // Binding power: below ||/&& (6/8), above assignment (2).
                let left_bp = 4u8;
                if left_bp < min_bp {
                    break;
                }
                let lhs_start = self.find_previous_expr_start_after(expr_start);
                // Consume both ? tokens, emitting a single QUESTION_QUESTION node
                self.bump(); // first ?
                self.bump(); // second ?
                self.parse_expr_bp(5); // right_bp = 5 (left associative)
                self.wrap_events_in_node(lhs_start, SyntaxKind::BINARY_EXPR);
                self.finish_node();
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
            } else if op == TokenKind::QuestionDot {
                // Optional chaining: obj?.field, obj?.[expr], obj?.(args)
                let lhs_start = self.find_previous_expr_start_after(expr_start);
                if self.peek_after_question_dot() == Some(TokenKind::LParen) {
                    // obj?.(args) — optional call
                    self.wrap_events_in_node(lhs_start, SyntaxKind::OPTIONAL_CALL_EXPR);
                    self.bump(); // ?.
                    self.parse_call_args();
                    self.finish_node();
                } else if self.peek_after_question_dot() == Some(TokenKind::LBracket) {
                    // obj?.[expr] — optional index
                    self.wrap_events_in_node(lhs_start, SyntaxKind::OPTIONAL_INDEX_EXPR);
                    self.bump(); // ?.
                    self.bump(); // [
                    self.parse_expr();
                    self.expect(TokenKind::RBracket);
                    self.finish_node();
                } else {
                    // obj?.field — optional field access
                    self.wrap_events_in_node(lhs_start, SyntaxKind::OPTIONAL_FIELD_ACCESS_EXPR);
                    self.bump(); // ?.
                    if self.at(TokenKind::Word) {
                        self.bump();
                    } else {
                        self.error_unexpected_token(
                            "Expected field name, '[', or '(' after '?.'".to_string(),
                        );
                    }
                    self.finish_node();
                }
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
                // by checking if we've emitted any events since expr_start.
                //
                // `suppress_object_literal_depth > 0` is set by `parse_spawn_expr`
                // while parsing the optional name expression — without it,
                // `spawn nm { y: 1 }` would consume the body's `{ y: 1 }` as
                // a struct literal and then fail to find a body brace.
                if self.suppress_object_literal_depth == 0
                    && self.events.len() > expr_start
                    && self.looks_like_object_constructor()
                {
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
            } else if op == TokenKind::Is {
                // `<expr> is <pattern>` — Rust `matches!`-style pattern test.
                // Same binding power as comparison operators (18, 19).
                let left_bp = 18u8;
                if left_bp < min_bp {
                    break;
                }
                let lhs_start = self.find_previous_expr_start_after(expr_start);
                self.bump(); // is
                self.parse_pattern();
                self.wrap_events_in_node(lhs_start, SyntaxKind::IS_EXPR);
                self.finish_node();
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

    /// Peek at the next non-trivia token after the current `?.` token.
    /// Used to disambiguate `?.field` vs `?.[expr]` vs `?.(args)`.
    fn peek_after_question_dot(&self) -> Option<TokenKind> {
        self.peek(1).map(|t| t.kind)
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
        // Two conditions must hold:
        // 1. The preceding expression looks like a type name (Word, PATH_EXPR, etc.)
        // 2. The content after `{` looks like `<word> :` (field initializer),
        //    not arbitrary statements. This prevents `Name { body_code }` from
        //    being mis-parsed as an object literal.

        if !self.brace_content_looks_like_fields() {
            return false;
        }

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
                Event::Token { kind, text, .. } => {
                    if depth == 0 {
                        // Only WORD tokens can be type names, and literals
                        // like null/true/false are never constructors.
                        return *kind == SyntaxKind::WORD
                            && !matches!(text.as_str(), "null" | "true" | "false");
                    }
                }
                Event::UnexpectedToken { .. } | Event::SyntaxHint { .. } => {}
            }
        }
        false
    }

    /// Peek past the current `{` token to check if the brace content starts with
    /// `<word> :` or `...` (spread), which indicates an object literal / constructor.
    /// If it starts with something else (e.g. a statement, keyword, or expression),
    /// the `{` is more likely a block body.
    fn brace_content_looks_like_fields(&self) -> bool {
        // peek() already skips trivia (whitespace, newlines, comments),
        // so peek(1) is the first content token after `{`.
        match self.peek(1) {
            Some(t) if t.kind == TokenKind::DotDotDot => true, // spread
            Some(t) if t.kind == TokenKind::RBrace => true,    // empty braces
            Some(t) if t.kind == TokenKind::Word => {
                // Check for `<word> :` pattern
                self.peek(2)
                    .map(|t| t.kind == TokenKind::Colon)
                    .unwrap_or(false)
            }
            _ => false,
        }
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

    /// Parse prefix expression (primary or unary operator).
    ///
    /// Unary operators (`!`, `-`, `~`, `++`, `--`) bind looser than postfix
    /// operators (`.`, `()`, `[]`), so `!x.f()` parses as `!(x.f())`.
    /// We achieve this by parsing the operand with `parse_expr_bp(PREFIX_BP)`
    /// instead of recursing into `parse_prefix()` directly — the Pratt loop
    /// then handles `.` and `()` before the `UNARY_EXPR` node closes.
    fn parse_prefix(&mut self) {
        // Binding power for unary prefix operators — higher than all infix
        // operators (max infix is 22) so `!a + b` is still `(!a) + b`, but
        // the operand goes through parse_expr_bp so postfix `.`/`()`/`[]`
        // bind tighter.
        const PREFIX_BP: u8 = 23;

        // Check for unary operators
        if self.at(TokenKind::Minus)
            || self.at(TokenKind::Not)
            || self.at(TokenKind::Tilde)
            || self.at(TokenKind::PlusPlus)
            || self.at(TokenKind::MinusMinus)
        {
            self.with_node(SyntaxKind::UNARY_EXPR, |p| {
                p.bump(); // operator
                p.parse_expr_bp(PREFIX_BP); // operand: postfix ops bind tighter
            });
        } else if self.at(TokenKind::Await) {
            // BEP-034 `await expr` — prefix operator binding like other
            // prefixes so postfix `.`/`()`/`[]` still attach to the
            // awaited value.
            self.with_node(SyntaxKind::AWAIT_EXPR, |p| {
                p.bump(); // `await`
                p.parse_expr_bp(PREFIX_BP);
            });
        } else if self.at(TokenKind::Spawn) {
            // BEP-034 `spawn name_expr? block`.
            self.parse_spawn_expr();
        } else {
            self.parse_primary_expr();
        }
    }

    /// Parse `spawn name_expr? { body }`. The name expression is optional
    /// and is parsed until we see `{` (v1 has no `with` clause). The body
    /// is always a brace-delimited block.
    fn parse_spawn_expr(&mut self) {
        self.with_node(SyntaxKind::SPAWN_EXPR, |p| {
            p.bump(); // `spawn`
            // Optional name expression: anything that can lead an
            // expression and is not `{`. We parse the name with a binding
            // power of 0 (no infix beyond what naturally terminates at
            // `{`), then the brace-block.
            if !p.at(TokenKind::LBrace) {
                // Suppress object-literal postfix so the body brace
                // isn't consumed as a struct constructor — without
                // this, `spawn nm { y: 1 }` parses `nm { y: 1 }` as
                // an OBJECT_LITERAL and the body is missing.
                p.suppress_object_literal_depth += 1;
                p.parse_expr_bp(1);
                p.suppress_object_literal_depth -= 1;
            }
            if p.at(TokenKind::LBrace) {
                p.parse_block_expr();
            } else {
                p.error_unexpected_token("'{' after spawn".to_string());
            }
        });
    }

    /// Parse primary expression (literals, identifiers, parentheses)
    fn parse_primary_expr(&mut self) {
        if self.at(TokenKind::IntegerLiteral) || self.at(TokenKind::FloatLiteral) {
            // Numeric literal
            self.bump();
        } else if self.parse_any_string() {
            // String literal
        } else if self.at(TokenKind::Throw) {
            // Throw expression
            self.parse_throw_expr();
        } else if self.at(TokenKind::Word) {
            // Collect text as owned String so the borrow is released before any &mut calls.
            let text: String = self.current().map(|t| t.text.clone()).unwrap_or_default();
            if text == "b" && self.parse_byte_string() {
                // Byte string literal b"..."
            } else if text == "env"
                && self.peek(1).map(|t| t.kind) == Some(TokenKind::Dot)
                && self.peek(2).map(|t| t.kind) == Some(TokenKind::Word)
                && self.peek(3).map(|t| t.kind) != Some(TokenKind::LParen)
            {
                // env.FIELD sugar (not followed by `(`) — desugar to baml.env.get_or_panic("FIELD")
                self.parse_env_access();
            } else if text == "true" || text == "false" {
                // Boolean literal
                self.bump();
            } else if text == "null" {
                // Null literal
                self.bump();
            } else {
                // Identifier or path (could be multi-segment like baml.HttpMethod.Get)
                self.parse_path_or_ident();
            }
        } else if self.at(TokenKind::Client) {
            // `client` is KW_CLIENT; allow as identifier (e.g. parameter named `client`, `client.execute(...)`)
            self.parse_path_or_ident();
        } else if self.at(TokenKind::LParen) && self.looks_like_lambda() {
            // Lambda expression: (params) -> [RetType] { body }
            self.parse_lambda_expr();
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
        } else if self.at(TokenKind::Word)
            && self.current().map(|t| t.text.as_str()) == Some("env")
            && self.peek(1).map(|t| t.kind) == Some(TokenKind::Dot)
        {
            // env.FIELD sugar
            self.parse_env_access();
        } else if self.at(TokenKind::Less) && self.looks_like_generic_lambda() {
            // Generic lambda expression: <T>(params) -> [RetType] { body }
            self.parse_lambda_expr();
        } else {
            self.error_unexpected_token("expression".to_string());
            // Consume the unexpected token to avoid infinite loops
            if !self.at_end() {
                self.bump();
            }
        }
    }

    /// Parse `env.FIELD` expressions.
    ///
    /// Produces an `ENV_ACCESS_EXPR` node: `WORD("env") DOT WORD`.
    /// AST lowering desugars to `baml.env.get_or_panic("FIELD")`.
    fn parse_env_access(&mut self) {
        self.with_node(SyntaxKind::ENV_ACCESS_EXPR, |p| {
            p.bump(); // consume `env` (Word)
            if p.eat(TokenKind::Dot) {
                if p.at(TokenKind::Word) {
                    p.bump(); // consume field name
                } else {
                    p.error_unexpected_token("identifier after 'env.'".to_string());
                }
            } else {
                p.error_unexpected_token("'.' after 'env'".to_string());
            }
        });
    }

    fn parse_call_args(&mut self) {
        self.with_node(SyntaxKind::CALL_ARGS, |p| {
            p.expect(TokenKind::LParen);

            if !p.at(TokenKind::RParen) {
                p.parse_call_arg();

                while p.eat(TokenKind::Comma) {
                    if p.at(TokenKind::RParen) {
                        break; // Trailing comma
                    }
                    p.parse_call_arg();
                }
            }

            p.expect(TokenKind::RParen);
        });
    }

    fn parse_call_arg(&mut self) {
        self.with_node(SyntaxKind::CALL_ARG, |p| {
            if (p.at(TokenKind::Word) || p.at(TokenKind::Client))
                && p.peek(1).map(|t| t.kind) == Some(TokenKind::Equals)
            {
                p.bump();
                p.expect(TokenKind::Equals);
            }
            p.parse_expr();
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

    /// Check if `<` starts generic arguments rather than a comparison.
    ///
    /// TypeScript-style disambiguation: scan ahead to balance `<...>`,
    /// allowing only tokens that could appear inside a type-argument list,
    /// and commit to generic only if the token after the closing `>` is one
    /// that can't follow a comparison expression — `(`, `{`, or `.`.
    ///
    /// Examples:
    ///   - `f<K, V>(x)`        → `>` followed by `(` → generic call
    ///   - `Box<int> { ... }`  → `>` followed by `{` → generic constructor
    ///   - `Wrapper<T>.of(x)`  → `>` followed by `.` → generic-qualified path
    ///   - `[a < b, c > d]`    → `>` followed by `d` (Word) → comparisons
    ///   - `a < b()`           → contains `(` inside → comparison
    fn looks_like_generic_args(&self) -> bool {
        if !self.at(TokenKind::Less) {
            return false;
        }

        let mut depth: i32 = 1;
        let mut i: usize = 1;
        loop {
            let Some(tok) = self.peek(i) else {
                return false;
            };
            match tok.kind {
                TokenKind::Less => depth += 1,
                TokenKind::Greater => {
                    depth -= 1;
                    if depth == 0 {
                        return Self::is_generic_args_follow(self.peek(i + 1).map(|t| t.kind));
                    }
                }
                // `>>` closes two levels at once. Only treat it as a dual
                // closer when there are actually two levels open — otherwise
                // it's a shift/comparison sequence (e.g. `a < b >> c`).
                TokenKind::GreaterGreater => {
                    if depth < 2 {
                        return false;
                    }
                    depth -= 2;
                    if depth == 0 {
                        return Self::is_generic_args_follow(self.peek(i + 1).map(|t| t.kind));
                    }
                }
                // Tokens that can legally appear inside a type-argument
                // list. Mirror the start tokens accepted by `parse_type` /
                // `parse_type_primary`:
                // - `Word` / `Dot` for type names and qualified paths
                // - `Comma` between args
                // - `LBracket` / `RBracket` for array suffix `T[]`
                // - `Question` for optional `T?`
                // - `Pipe` for unions `A | B`
                // - `IntegerLiteral` / `FloatLiteral` for literal-union members
                // - `Minus` to allow negative numeric literal types (`-1`)
                //   that `parse_type_primary` accepts as type atoms
                // - `Quote` / `Hash` for string-literal types (`"a"`,
                //   `#"raw"#`)
                // - `LParen` / `RParen` for parenthesized union types
                //   (`(int | string)`)
                TokenKind::Word
                | TokenKind::Dot
                | TokenKind::Comma
                | TokenKind::LBracket
                | TokenKind::RBracket
                | TokenKind::Question
                | TokenKind::Pipe
                | TokenKind::IntegerLiteral
                | TokenKind::FloatLiteral
                | TokenKind::Minus
                | TokenKind::Quote
                | TokenKind::Hash
                | TokenKind::LParen
                | TokenKind::RParen => {}
                // Anything else — operators, braces, EOF-ish tokens — can't
                // appear in a type, so this `<` is a comparison.
                _ => return false,
            }
            i += 1;
        }
    }

    fn is_generic_args_follow(kind: Option<TokenKind>) -> bool {
        matches!(
            kind,
            Some(TokenKind::LParen | TokenKind::LBrace | TokenKind::Dot)
        )
    }

    /// Parse generic arguments: <Type1, Type2, ...>
    fn parse_generic_args(&mut self) {
        self.type_args_depth += 1;
        self.with_node(SyntaxKind::GENERIC_ARGS, |p| {
            p.expect(TokenKind::Less);

            // Parse first type argument
            if !p.at(TokenKind::Greater) && !p.at(TokenKind::GreaterGreater) {
                p.parse_type();

                // Parse remaining type arguments
                while p.eat(TokenKind::Comma) {
                    if p.at(TokenKind::Greater) || p.at(TokenKind::GreaterGreater) {
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
            for _ in 0..self.pending_greaters {
                self.events.push(Event::Token {
                    kind: SyntaxKind::GREATER,
                    text: ">".to_string(),
                });
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

    /// Check if the current position starts a lambda expression.
    /// Disambiguates `(` in expression position between parenthesized expr and lambda.
    ///
    /// Positive signals:
    /// - `( )` followed by `->` → zero-param lambda
    /// - `( Word :` → typed param (tuples won't use `:` after identifiers)
    /// - `( ... ) ->` → depth-aware paren scan then check for `->` (untyped multi-param)
    fn looks_like_lambda(&self) -> bool {
        if !self.at(TokenKind::LParen) {
            return false;
        }

        // `( ) ->` or `( ) =>` → zero-param lambda
        if self.peek(1).map(|t| t.kind) == Some(TokenKind::RParen)
            && matches!(
                self.peek(2).map(|t| t.kind),
                Some(TokenKind::Arrow | TokenKind::FatArrow)
            )
        {
            return true;
        }

        // `( Word :` → typed param → definitely a lambda
        if self.peek(1).map(|t| t.kind) == Some(TokenKind::Word)
            && self.peek(2).map(|t| t.kind) == Some(TokenKind::Colon)
        {
            return true;
        }

        // Depth-aware scan: find matching `)`, then check for `->`
        // This handles `(a, b) -> { ... }` and `(a) -> { ... }`
        let mut depth: u32 = 0;
        let mut offset: usize = 0;
        loop {
            let Some(token) = self.peek(offset) else {
                return false;
            };
            match token.kind {
                TokenKind::LParen => depth += 1,
                TokenKind::RParen => {
                    depth -= 1;
                    if depth == 0 {
                        // Check if `->` or `=>` follows the closing `)`
                        return matches!(
                            self.peek(offset + 1).map(|t| t.kind),
                            Some(TokenKind::Arrow | TokenKind::FatArrow)
                        );
                    }
                }
                _ => {}
            }
            offset += 1;
            // Safety limit consistent with looks_like_catch_arm_start
            if offset > 64 {
                return false;
            }
        }
    }

    /// Check if `<` starts a generic lambda expression.
    /// `< Word >` or `< Word ,` followed by `(` → generic lambda.
    fn looks_like_generic_lambda(&self) -> bool {
        if !self.at(TokenKind::Less) {
            return false;
        }
        // `< Word > (` or `< Word , ... > (`
        if let Some(t1) = self.peek(1) {
            if t1.kind != TokenKind::Word {
                return false;
            }
            if let Some(t2) = self.peek(2) {
                if t2.kind == TokenKind::Greater {
                    // `< Word >` — check for `(`
                    return self.peek(3).map(|t| t.kind) == Some(TokenKind::LParen);
                }
                if t2.kind == TokenKind::Comma {
                    // `< Word ,` — multi-param generics, scan for closing `>`
                    let mut offset = 3;
                    loop {
                        match self.peek(offset) {
                            Some(t) if t.kind == TokenKind::Greater => {
                                return self.peek(offset + 1).map(|t| t.kind)
                                    == Some(TokenKind::LParen);
                            }
                            Some(_) => offset += 1,
                            None => return false,
                        }
                        if offset > 64 {
                            return false;
                        }
                    }
                }
            }
        }
        false
    }

    /// Parse a map literal in expression context: { "key": value, ... }
    /// Requires colons and commas (JSON-style)
    fn parse_map_literal(&mut self) {
        self.with_node(SyntaxKind::MAP_LITERAL, |p| {
            p.expect(TokenKind::LBrace);

            // Parse map entries
            while !p.at(TokenKind::RBrace) && !p.at_end() {
                if p.at_statement_recovery_boundary() {
                    break;
                }

                // Check for valid entry start
                if p.at(TokenKind::Word) || p.at(TokenKind::Quote) || p.at(TokenKind::Hash) {
                    p.parse_map_entry();

                    // Handle comma between entries
                    if !p.at(TokenKind::RBrace) {
                        if p.at_statement_recovery_boundary() {
                            break;
                        }
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
        if !self.at(TokenKind::Word) && !self.at(TokenKind::Client) {
            return;
        }

        let segment = |k: TokenKind| matches!(k, TokenKind::Word | TokenKind::Client);

        // Check if this looks like a path (ident.client followed by dot and another ident)
        if self.peek(1).map(|t| t.kind) == Some(TokenKind::Dot)
            && self.peek(2).map(|t| segment(t.kind)).unwrap_or(false)
        {
            // It's a path - all segments are identifiers
            self.with_node(SyntaxKind::PATH_EXPR, |p| {
                p.bump(); // First segment

                // Parse remaining segments
                while p.eat(TokenKind::Dot) {
                    if p.at(TokenKind::Word) || p.at(TokenKind::Client) {
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
            if p.at_statement_recovery_boundary() {
                p.error_unexpected_token("map value".to_string());
                return;
            }
            p.parse_expr();
        });
    }

    /// Parse the body of an object literal/constructor: { field: value, ...spread }
    fn parse_object_literal_body(&mut self) {
        self.expect(TokenKind::LBrace);

        // Parse fields until we hit the closing brace
        while !self.at(TokenKind::RBrace) && !self.at_end() {
            if self.at_statement_recovery_boundary() {
                break;
            }

            // Check for spread element: ...expr
            if self.at(TokenKind::DotDotDot) {
                self.parse_spread_element();

                // Handle comma between elements
                if !self.at(TokenKind::RBrace) {
                    if self.at_statement_recovery_boundary() {
                        break;
                    }
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
                    if self.at_statement_recovery_boundary() {
                        break;
                    }
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
            if p.at_statement_recovery_boundary() {
                p.error_unexpected_token("field value".to_string());
                return;
            }
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
            OrOr => (6, 7),

            // Logical AND (left associative)
            AndAnd => (8, 9),

            // Bitwise OR (left associative)
            Pipe => (10, 11),

            // Bitwise XOR (left associative)
            Caret => (12, 13),

            // Bitwise AND (left associative)
            And => (14, 15),

            // Equality (left associative)
            EqualsEquals | NotEquals => (16, 17),

            // Comparison (left associative) - includes instanceof
            Less | Greater | LessEquals | GreaterEquals | Instanceof => (18, 19),

            // Bitwise shift (left associative)
            LessLess | GreaterGreater => (20, 21),

            // Addition/Subtraction (left associative)
            Plus | Minus => (22, 23),

            // Multiplication/Division/Modulo (left associative)
            Star | Slash | Percent => (24, 25),

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
                p.type_args_depth += 1;
                p.with_node(SyntaxKind::CLIENT_TYPE, |p| {
                    p.bump(); // <
                    if p.at(TokenKind::Word) {
                        p.bump(); // type name
                    }
                    p.expect_greater(); // >
                });
                p.type_args_depth -= 1;

                if p.pending_greaters > 0 {
                    if let Some(span) = p.pending_greater_span {
                        p.error(
                            format!(
                                "Unmatched '>' in client definition (found {} extra)",
                                p.pending_greaters
                            ),
                            span,
                        );
                    }
                    for _ in 0..p.pending_greaters {
                        p.events.push(Event::Token {
                            kind: SyntaxKind::GREATER,
                            text: ">".to_string(),
                        });
                    }
                    p.pending_greaters = 0;
                    p.pending_greater_span = None;
                }
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

                // Block attributes (e.g. `@@some_attr(...)`) inside config blocks
                if p.at(TokenKind::AtAt) {
                    p.parse_atat_attribute();
                } else {
                    p.parse_config_item();
                    // Allow optional comma after config items
                    p.eat(TokenKind::Comma);
                }
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

            // Optional colon
            p.eat(TokenKind::Colon);

            // Config value - can be nested block or simple value
            if p.at(TokenKind::LBrace) {
                // Nested config block
                p.parse_config_block();
            } else {
                // Simple value - unquoted string or other expression
                p.parse_config_value();
            }

            // Optional field attributes after config value (e.g., args { ... } @some_attr(...))
            while p.at(TokenKind::At) && !p.at(TokenKind::AtAt) {
                p.parse_at_attribute();
            }
        });
    }

    /// Parse a `type_builder` block inside a test definition.
    /// Contains class, enum, dynamic class, dynamic enum, and type alias definitions.
    fn parse_type_builder_block(&mut self) {
        self.with_node(SyntaxKind::TYPE_BUILDER_BLOCK, |p| {
            p.expect(TokenKind::TypeBuilder);

            // Optional colon
            p.eat(TokenKind::Colon);

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
                p.error_unexpected_token(
                    "Incomplete 'dynamic' type definition. Use 'dynamic class' or 'dynamic enum' to add properties to types that contain the `@@dynamic` attribute.".to_string()
                );
            }
        });
    }

    fn parse_config_value(&mut self) {
        self.with_node(SyntaxKind::CONFIG_VALUE, |p| {
            // Config values can be:
            // - Strings: "value", #"value"#
            // - Arrays: [item1, item2]
            // - Nested blocks: { key: value }
            // - Expressions: env.MY_MODEL, "Bearer " + env.FOO_KEY
            // - Numbers: 123, 3.14
            // - Unquoted strings (legacy): gpt-4o, path/to/file

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

            // Check if this looks like an expression that should be parsed as such:
            // - String literals (quoted)
            // - Numbers (integer or float literals)
            // - `env.` prefix (environment variable access)
            // - `true` / `false` (boolean literals)
            if p.looks_like_config_expression() {
                p.parse_expr();
            } else {
                // Unquoted string: multi-word is no longer allowed
                p.expect(TokenKind::Word);
            }
        });
    }

    /// Check if the current position looks like an expression that should be parsed
    /// as such, rather than as a legacy unquoted string.
    fn looks_like_config_expression(&self) -> bool {
        // Regular string literals are always expressions
        if self.at(TokenKind::Quote) {
            return true;
        }

        // Block strings start with #", ##", etc.
        // (Just # alone like `#helloworld` is a legacy unquoted string)
        if self.at(TokenKind::Hash) {
            let num_hashes = self.count_consecutive_hashes();
            if let Some(next) = self.find_token_after_hashes(num_hashes) {
                return self.tokens[next].kind == TokenKind::Quote;
            }
            return false;
        }

        // Number literals
        if self.at(TokenKind::IntegerLiteral) || self.at(TokenKind::FloatLiteral) {
            return true;
        }
        if self.at(TokenKind::Minus)
            && self.peek(1).is_some_and(|t| {
                matches!(t.kind, TokenKind::IntegerLiteral | TokenKind::FloatLiteral)
            })
        {
            return true;
        }

        // Byte string literals: b"..."
        if self.at(TokenKind::Word)
            && let Some(token) = self.current()
            && token.text.as_str() == "b"
            && let Some(idx) = self.current_non_trivia_index()
            && self
                .tokens
                .get(idx + 1)
                .is_some_and(|t| t.kind == TokenKind::Quote)
        {
            return true;
        }

        // Check for `env.` prefix - environment variable access
        if self.at(TokenKind::Word) {
            if let Some(token) = self.current() {
                if token.text.as_str() == "env" {
                    if let Some(next) = self.peek(1) {
                        if next.kind == TokenKind::Dot {
                            return true;
                        }
                    }
                }
            }
        }

        // Boolean literals
        if self.at(TokenKind::Word) {
            if let Some(token) = self.current() {
                let text = token.text.as_str();
                if text == "true" || text == "false" {
                    return true;
                }
            }
        }

        false
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

    /// Check if the current test looks like an expression-body test (new-style).
    ///
    /// Old-style: `test Name { functions [...] ... }` — config block
    /// New-style: `test <expr> [with <expr>] { ... }` — expression body
    ///
    /// We detect the old style and default to new style otherwise, so that
    /// any expression (string concat, function calls, etc.) works as a test name.
    fn looks_like_test_expr_body(&self) -> bool {
        let Some(next) = self.peek(1) else {
            return true;
        };
        // Old-style is only: `test Name { functions ...}` or `test Name { type_builder ...}`
        // There is no old-style form with parens — `test Name(...)` was never valid old-style
        // syntax (the old parser just emits a "remove parentheses" error).
        if next.kind == TokenKind::Word {
            if let Some(after_word) = self.peek(2) {
                if after_word.kind == TokenKind::LBrace {
                    // Peek inside the brace: `test Name { functions` or `test Name { type_builder`
                    if let Some(inside) = self.peek(3) {
                        if inside.kind == TokenKind::Word
                            && (inside.text == "functions" || inside.text == "type_builder")
                        {
                            return false; // old-style config block
                        }
                    }
                }
            }
        }
        true
    }

    /// Parse an expression-body test: `test <name_expr> [with expr] { body }`
    ///
    /// The name is any expression that type-checks as a string, e.g.:
    /// - `test "simple" { ... }`
    /// - `test "prefix" + suffix { ... }`
    pub(crate) fn parse_test_expr(&mut self) {
        self.with_node(SyntaxKind::TEST_EXPR_DEF, |p| {
            // 'test' keyword
            p.expect(TokenKind::Test);

            // Test name — an expression (stops before `{` and `with`)
            p.parse_expr();

            // Optional `with` clause for test runner
            if p.at_contextual_kw("with") {
                p.bump_contextual_with();
                p.parse_expr();
            }

            // Block body — reuse existing expression body parsing
            if p.at(TokenKind::LBrace) {
                p.parse_block_expr();
            } else {
                p.error_unexpected_token("test body".to_string());
            }
        });
    }

    /// Parse a testset: `testset "name" [with expr] { body }`
    /// Body can contain statements, nested `test` and `testset` blocks.
    pub(crate) fn parse_testset(&mut self) {
        self.with_node(SyntaxKind::TESTSET_DEF, |p| {
            // 'testset' keyword
            p.expect(TokenKind::TestSet);

            // Testset name — an expression (stops before `{` and `with`)
            p.parse_expr();

            // Optional `with` clause for testset runner
            if p.at_contextual_kw("with") {
                p.bump_contextual_with();
                p.parse_expr();
            }

            // Block body — parse as a block containing statements + nested test/testset
            if p.at(TokenKind::LBrace) {
                p.parse_testset_body();
            } else {
                p.error_unexpected_token("testset body".to_string());
            }
        });
    }

    /// Parse the body of a testset block.
    /// Allows statements (let, for) and nested test/testset declarations.
    fn parse_testset_body(&mut self) {
        self.testset_body_depth += 1;
        self.parse_block_expr();
        self.testset_body_depth -= 1;
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

            // Optional attributes (not including those taken by the type)
            while p.at(TokenKind::At) && !p.at(TokenKind::AtAt) {
                p.parse_at_attribute();
            }

            // Optional semicolon
            p.eat(TokenKind::Semicolon);
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
        let attributed_item = if parser.at(TokenKind::AtAt) {
            parser.item_keyword_after_leading_block_attributes()
        } else {
            None
        };

        if parser.at(TokenKind::Enum) || attributed_item == Some(TokenKind::Enum) {
            parser.parse_enum();
        } else if parser.at(TokenKind::Class) || attributed_item == Some(TokenKind::Class) {
            parser.parse_class();
        } else if parser.at(TokenKind::Function) || attributed_item == Some(TokenKind::Function) {
            parser.parse_function();
        } else if parser.at(TokenKind::Client) {
            parser.parse_client();
        } else if parser.at(TokenKind::Generator) {
            parser.parse_generator();
        } else if parser.at(TokenKind::Test) {
            if parser.looks_like_test_expr_body() {
                parser.parse_test_expr();
            } else {
                parser.parse_test();
            }
        } else if parser.at(TokenKind::TestSet) {
            parser.parse_testset();
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
        if parser.at_header_comment_start() {
            parser.consume_header_comment();
        } else if parser.at_line_comment_start() {
            parser.consume_line_comment();
        } else if parser.at_block_comment_start() {
            parser.consume_block_comment();
        } else {
            let token = &parser.tokens[parser.current];
            let kind = token_kind_to_syntax_kind(token.kind);
            parser.events.push(Event::Token {
                kind,
                text: token.text.clone(),
            });
            parser.current += 1;
        }
    }

    parser.finish_node();

    parser.build_tree(cache)
}

#[cfg(test)]
mod tests {
    use baml_base::FileId;
    use baml_compiler_lexer::lex_lossless;
    use baml_compiler_syntax::{SyntaxKind, SyntaxNode};

    use super::{ParseError, parse_file};

    fn parse_source(source: &str) -> (SyntaxNode, Vec<ParseError>) {
        let tokens = lex_lossless(source, FileId::new(0));
        let (green, errors) = parse_file(&tokens);
        (SyntaxNode::new_root(green), errors)
    }

    fn assert_no_errors(errors: &[ParseError]) {
        assert!(
            errors.is_empty(),
            "expected no parse errors, got: {errors:#?}"
        );
    }

    #[test]
    fn is_expr_parses_at_comparison_precedence() {
        // `<expr> is <pattern>` should produce an IS_EXPR node, parsed at the
        // same binding power as comparison operators. Verifies the parser
        // accepts a bare type-pattern RHS, an or-pattern RHS, and `is` chained
        // with `&&` (where `&&` is lower precedence so wraps both `is` nodes).
        let source =
            "function f() -> bool {\n  let v: int | string = 1\n  v is int && v is int | bool\n}\n";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        let is_count = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::IS_EXPR)
            .count();
        assert_eq!(is_count, 2, "expected two IS_EXPR nodes");
    }

    #[test]
    fn accepts_parameter_type_without_colon() {
        // BEP-019: colons are optional in function parameters.
        // `x int` is valid syntax (formatter will add the colon).
        let source = r#"
function Demo(x int) -> int {
  x
}
"#;

        let (root, errors) = parse_source(source);

        // No errors expected - colons are optional
        assert_no_errors(&errors);

        // The parameter node should contain the type
        let param = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::PARAMETER)
            .expect("expected PARAMETER node");
        let param_text = param.text().to_string();
        assert!(
            param_text.contains("int"),
            "parameter should contain the type 'int', got: {param_text:?}"
        );
    }

    #[test]
    fn parses_top_level_function_with_leading_block_attributes() {
        let source = r#"
@@internal.uses(engine_ctx)
@@internal.panics(HostPanic)
function fetch(url: string) -> string throws baml.errors.Io | baml.errors.Timeout {
  $rust_io_function
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let func = root
            .children()
            .find(|n| n.kind() == SyntaxKind::FUNCTION_DEF)
            .expect("expected FUNCTION_DEF");
        let attrs: Vec<_> = func
            .children()
            .filter(|n| n.kind() == SyntaxKind::BLOCK_ATTRIBUTE)
            .collect();

        assert_eq!(attrs.len(), 2, "expected two function block attributes");
    }

    #[test]
    fn parses_method_with_leading_block_attributes() {
        let source = r#"
class Response {
  @@internal.uses(engine_ctx)
  function text(self) -> string throws baml.errors.Io {
    $rust_io_function
  }
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let class = root
            .children()
            .find(|n| n.kind() == SyntaxKind::CLASS_DEF)
            .expect("expected CLASS_DEF");
        let func = class
            .children()
            .find(|n| n.kind() == SyntaxKind::FUNCTION_DEF)
            .expect("expected FUNCTION_DEF in class");
        let attrs: Vec<_> = func
            .children()
            .filter(|n| n.kind() == SyntaxKind::BLOCK_ATTRIBUTE)
            .collect();

        assert_eq!(attrs.len(), 1, "expected method block attribute");
    }

    #[test]
    fn raw_string_keeps_comment_markers_as_text() {
        for marker in ["//", "*/"] {
            let source = format!(
                r##"
function Demo() -> string {{
  #"{marker}"#
}}
"##
            );

            let (root, errors) = parse_source(&source);
            assert_no_errors(&errors);

            let raw_string = root
                .descendants()
                .find(|n| n.kind() == SyntaxKind::RAW_STRING_LITERAL)
                .expect("expected raw string literal");
            assert!(
                raw_string.text().to_string().contains(marker),
                "raw string should retain marker {marker:?}: {raw_string:?}"
            );
        }
    }

    #[test]
    fn incomplete_map_field_value_stops_before_watch_let() {
        let (root, _errors) = parse_source(
            r#"
function GuessGameAgent() -> string {
  log.info({"famous_person_name":
  watch let user_input = SimulateHumanGuess(history)
  user_input
}
"#,
        );

        let map = root
            .descendants()
            .find(|node| node.kind() == SyntaxKind::MAP_LITERAL)
            .expect("map literal should still be parsed");
        assert!(
            !map.text().to_string().contains("watch let"),
            "unterminated map literal swallowed the following watched statement: {}",
            map.text()
        );

        assert!(
            root.descendants()
                .any(|node| node.kind() == SyntaxKind::WATCH_LET),
            "`watch let` should recover as its own statement"
        );
    }

    #[test]
    fn parses_function_with_client_as_parameter_name() {
        // `client` is a keyword (KW_CLIENT); it must still be valid as a parameter name.
        let source = r#"
function call_llm_function(client: Client, function_name: string) -> unknown {
  let _ = client
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let func = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::FUNCTION_DEF)
            .expect("expected FUNCTION_DEF");
        let type_exprs: Vec<_> = func
            .children()
            .filter(|n| n.kind() == SyntaxKind::TYPE_EXPR)
            .collect();
        assert_eq!(
            type_exprs.len(),
            1,
            "expected one top-level return TYPE_EXPR on FUNCTION_DEF"
        );
        let expr_body = func
            .children()
            .find(|n| n.kind() == SyntaxKind::EXPR_FUNCTION_BODY);
        assert!(
            expr_body.is_some(),
            "expected EXPR_FUNCTION_BODY on FUNCTION_DEF"
        );
    }

    #[test]
    fn expression_body_starting_with_client_dot_path_is_not_llm_body() {
        let source = r#"
function f() -> int {
  client.execute()
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let func = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::FUNCTION_DEF)
            .expect("expected FUNCTION_DEF");
        assert!(
            func.children()
                .any(|n| n.kind() == SyntaxKind::EXPR_FUNCTION_BODY),
            "expected expression body, not LLM body"
        );
    }

    #[test]
    fn expression_body_header_comment_with_prompt_word_is_not_llm_body() {
        let source = r#"
function f() -> string {
  //# Generate an image from the user prompt.
  let value = "ok"
  value
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let func = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::FUNCTION_DEF)
            .expect("expected FUNCTION_DEF");
        assert!(
            func.children()
                .any(|n| n.kind() == SyntaxKind::EXPR_FUNCTION_BODY),
            "expected expression body, not LLM body"
        );
    }

    #[test]
    fn missing_return_type_body_with_client_word_is_not_llm_body() {
        let source = r#"
function Foo() -> {
  client GPT4
}
"#;

        let (root, _errors) = parse_source(source);

        let func = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::FUNCTION_DEF)
            .expect("expected FUNCTION_DEF");
        assert!(
            func.children()
                .any(|n| n.kind() == SyntaxKind::EXPR_FUNCTION_BODY),
            "expected expression body, not LLM body"
        );
    }

    #[test]
    fn accepts_parameter_parenthesized_type_without_colon() {
        // BEP-019: colons are optional in function parameters.
        // `x (int | string)` is valid syntax.
        let source = r#"
function Demo(x (int | string)) -> int {
  1
}
"#;

        let (root, errors) = parse_source(source);

        assert_no_errors(&errors);

        let param = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::PARAMETER)
            .expect("expected PARAMETER node");
        let param_text = param.text().to_string();
        assert!(
            param_text.contains("int"),
            "parameter should contain parsed type, got: {param_text:?}"
        );
    }

    #[test]
    fn accepts_parameter_string_literal_type_without_colon() {
        // BEP-019: colons are optional in function parameters.
        // `x "hello"` is valid syntax.
        let source = r#"
function Demo(x "hello") -> int {
  1
}
"#;

        let (root, errors) = parse_source(source);

        assert_no_errors(&errors);

        let param = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::PARAMETER)
            .expect("expected PARAMETER node");
        let param_text = param.text().to_string();
        assert!(
            param_text.contains("hello"),
            "parameter should contain parsed type, got: {param_text:?}"
        );
    }

    #[test]
    fn accepts_parameter_raw_string_type_without_colon() {
        // BEP-019: colons are optional in function parameters.
        // `x #"hello"#` is valid syntax.
        let source = r##"
function Demo(x #"hello"#) -> int {
  1
}
"##;

        let (root, errors) = parse_source(source);

        assert_no_errors(&errors);

        let param = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::PARAMETER)
            .expect("expected PARAMETER node");
        let param_text = param.text().to_string();
        assert!(
            param_text.contains("hello"),
            "parameter should contain parsed type, got: {param_text:?}"
        );
    }

    #[test]
    fn accepts_parameter_integer_literal_type_without_colon() {
        // BEP-019: colons are optional in function parameters.
        // `x 200` is valid syntax.
        let source = r#"
function Demo(x 200) -> int {
  1
}
"#;

        let (root, errors) = parse_source(source);

        assert_no_errors(&errors);

        let param = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::PARAMETER)
            .expect("expected PARAMETER node");
        let param_text = param.text().to_string();
        assert!(
            param_text.contains("200"),
            "parameter should contain parsed type, got: {param_text:?}"
        );
    }

    #[test]
    fn parses_throw_statement_and_throw_expression_in_catch_arm() {
        let source = r#"
function Demo() -> int {
  throw err;
  foo() catch (e) {
    other => throw other
  }
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let throw_stmt = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::THROW_STMT)
            .expect("expected THROW_STMT node");
        assert!(
            throw_stmt
                .children()
                .any(|child| child.kind() == SyntaxKind::THROW_EXPR)
        );

        let throw_expr_count = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::THROW_EXPR)
            .count();
        assert_eq!(throw_expr_count, 2);
    }

    #[test]
    fn parses_catch_on_throw_expression_with_expected_binding() {
        let source = r#"
function Demo() -> int {
  throw 1 catch (e) {
    _ => 1
  }
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let catch_expr = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::CATCH_EXPR)
            .expect("expected CATCH_EXPR node");
        let child_kinds: Vec<_> = catch_expr.children().map(|n| n.kind()).collect();
        assert_eq!(child_kinds[0], SyntaxKind::THROW_EXPR);
        assert_eq!(child_kinds[1], SyntaxKind::CATCH_CLAUSE);
    }

    #[test]
    fn parses_return_throw_catch_expression() {
        let source = r#"
function Demo() -> int {
  return throw 1 catch (e) {
    _ => 2
  };
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let return_stmt = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::RETURN_STMT)
            .expect("expected RETURN_STMT node");

        let catch_expr = return_stmt
            .descendants()
            .find(|n| n.kind() == SyntaxKind::CATCH_EXPR)
            .expect("expected CATCH_EXPR under RETURN_STMT");

        let child_kinds: Vec<_> = catch_expr.children().map(|n| n.kind()).collect();
        assert_eq!(child_kinds[0], SyntaxKind::THROW_EXPR);
        assert_eq!(child_kinds[1], SyntaxKind::CATCH_CLAUSE);
    }

    #[test]
    fn parses_function_type_throws_clause() {
        let source = r#"
type Callback = (value: int) -> string throws Foo
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let function_type =
            root.descendants()
                .find(|n| {
                    n.kind() == SyntaxKind::TYPE_EXPR && n.children_with_tokens().any(|child| {
                        matches!(
                            child,
                            rowan::NodeOrToken::Token(token) if token.kind() == SyntaxKind::ARROW
                        )
                    })
                })
                .expect("expected function TYPE_EXPR");

        let throws = function_type
            .children()
            .find(|n| n.kind() == SyntaxKind::THROWS_CLAUSE)
            .expect("expected THROWS_CLAUSE under function type");

        assert!(
            throws.text().to_string().contains("throws Foo"),
            "expected function type throws clause text, got {:?}",
            throws.text().to_string()
        );
    }

    // ============ Pattern parsing ============

    /// Find the first `PATTERN` node directly under any `LET_STMT` in the tree.
    fn first_let_pattern(root: &SyntaxNode) -> SyntaxNode {
        root.descendants()
            .find(|n| n.kind() == SyntaxKind::LET_STMT)
            .expect("expected LET_STMT")
            .children()
            .find(|n| n.kind() == SyntaxKind::PATTERN)
            .expect("expected PATTERN under LET_STMT")
    }

    /// Direct child node-kinds of `node` (tokens stripped).
    fn child_kinds(node: &SyntaxNode) -> Vec<SyntaxKind> {
        node.children().map(|n| n.kind()).collect()
    }

    #[test]
    fn pattern_destructure_basic_with_let() {
        // `let Class { field } = ...` — destructure with a `let` prefix at
        // the let-statement level. The chain link is a single
        // DESTRUCTURE_PATTERN containing the `let` token.
        let source = r#"
function Demo() -> int {
  let Class { field } = obj;
  1
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let pattern = first_let_pattern(&root);
        // No chain — single atom.
        assert_eq!(child_kinds(&pattern), vec![SyntaxKind::DESTRUCTURE_PATTERN]);

        let destructure = pattern
            .children()
            .find(|n| n.kind() == SyntaxKind::DESTRUCTURE_PATTERN)
            .unwrap();

        // The `let` keyword should be inside the destructure node.
        let has_let = destructure.children_with_tokens().any(|c| {
            matches!(
                c,
                rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::KW_LET
            )
        });
        assert!(has_let, "destructure should contain a leading KW_LET");

        let fields: Vec<_> = destructure
            .children()
            .filter(|n| n.kind() == SyntaxKind::FIELD_PATTERN)
            .collect();
        assert_eq!(fields.len(), 1);
        assert!(fields[0].text().to_string().contains("field"));
    }

    #[test]
    fn pattern_destructure_many_fields() {
        let source = r#"
function Demo() -> int {
  let Class { a, b, c, d, e } = obj;
  1
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let pattern = first_let_pattern(&root);
        let destructure = pattern
            .children()
            .find(|n| n.kind() == SyntaxKind::DESTRUCTURE_PATTERN)
            .expect("expected DESTRUCTURE_PATTERN");

        let fields: Vec<_> = destructure
            .children()
            .filter(|n| n.kind() == SyntaxKind::FIELD_PATTERN)
            .collect();
        assert_eq!(fields.len(), 5);
    }

    #[test]
    fn pattern_destructure_with_inner_let_rename() {
        // `Class { field: let renamed }` — explicit rename via inner `let`.
        // The field's value is a PATTERN containing a BINDING_PATTERN.
        let source = r#"
function Demo() -> int {
  let Class { field: let renamed } = obj;
  1
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let pattern = first_let_pattern(&root);
        let destructure = pattern
            .children()
            .find(|n| n.kind() == SyntaxKind::DESTRUCTURE_PATTERN)
            .unwrap();
        let field = destructure
            .children()
            .find(|n| n.kind() == SyntaxKind::FIELD_PATTERN)
            .expect("expected FIELD_PATTERN");

        let inner_pattern = field
            .children()
            .find(|n| n.kind() == SyntaxKind::PATTERN)
            .expect("expected PATTERN inside FIELD_PATTERN");
        assert_eq!(
            child_kinds(&inner_pattern),
            vec![SyntaxKind::BINDING_PATTERN],
            "field value should be a single BINDING_PATTERN (`let renamed`)"
        );
    }

    #[test]
    fn pattern_destructure_field_typed() {
        // `Class { field: int }` — field value is a TYPE_PATTERN. Field-level
        // `:` is consumed by parse_field_pattern, NOT by the chain parser, so
        // the inner pattern has no CHAIN_PATTERN wrapper.
        let source = r#"
function Demo() -> int {
  let Class { field: int } = obj;
  1
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let pattern = first_let_pattern(&root);
        let field = pattern
            .descendants()
            .find(|n| n.kind() == SyntaxKind::FIELD_PATTERN)
            .unwrap();
        let inner_pattern = field
            .children()
            .find(|n| n.kind() == SyntaxKind::PATTERN)
            .unwrap();
        assert_eq!(child_kinds(&inner_pattern), vec![SyntaxKind::TYPE_PATTERN],);
    }

    #[test]
    fn pattern_destructure_nested() {
        // `Class { field: Class2 { field2 } }` — nested destructure.
        let source = r#"
function Demo() -> int {
  let Class { field: Class2 { field2 } } = obj;
  1
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let pattern = first_let_pattern(&root);
        let destructures: Vec<_> = pattern
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::DESTRUCTURE_PATTERN)
            .collect();
        assert_eq!(destructures.len(), 2, "expected outer + inner destructure");

        // The inner destructure should be reachable through a FIELD_PATTERN
        // > PATTERN inside the outer one.
        let outer = &destructures[0];
        let inner_via_field = outer
            .descendants()
            .find(|n| n.kind() == SyntaxKind::FIELD_PATTERN)
            .and_then(|f| f.children().find(|n| n.kind() == SyntaxKind::PATTERN))
            .and_then(|p| {
                p.children()
                    .find(|n| n.kind() == SyntaxKind::DESTRUCTURE_PATTERN)
            })
            .expect("inner destructure should sit under outer FIELD_PATTERN > PATTERN");
        assert!(inner_via_field.text().to_string().contains("field2"));
    }

    #[test]
    fn pattern_destructure_in_match_arm_no_let() {
        // In match arms, `Class { field }` does NOT require `let` — the
        // ambiguity-with-expressions argument doesn't apply there.
        let source = r#"
function Demo() -> int {
  match (x) {
    Class { field } => field,
    _ => 0,
  }
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let arms: Vec<_> = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::MATCH_ARM)
            .collect();
        assert_eq!(arms.len(), 2);

        // First arm's pattern is a DESTRUCTURE_PATTERN with no KW_LET token.
        let first_pattern = arms[0]
            .children()
            .find(|n| n.kind() == SyntaxKind::PATTERN)
            .expect("expected PATTERN in match arm");
        let destructure = first_pattern
            .children()
            .find(|n| n.kind() == SyntaxKind::DESTRUCTURE_PATTERN)
            .expect("expected DESTRUCTURE_PATTERN");
        let has_let = destructure.children_with_tokens().any(|c| {
            matches!(
                c,
                rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::KW_LET
            )
        });
        assert!(
            !has_let,
            "match-arm destructure should NOT have a leading `let`"
        );
    }

    #[test]
    fn pattern_array_slots_are_normal_patterns() {
        let source = r#"
function Demo() -> int {
  match (x) {
    [let first, string, ..let rest] => 1
  }
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let arm = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::MATCH_ARM)
            .expect("expected MATCH_ARM");
        let pattern = arm
            .children()
            .find(|n| n.kind() == SyntaxKind::PATTERN)
            .expect("expected PATTERN");
        let array = pattern
            .children()
            .find(|n| n.kind() == SyntaxKind::ARRAY_PATTERN)
            .expect("expected ARRAY_PATTERN");
        let elements: Vec<_> = array
            .children()
            .filter(|n| n.kind() == SyntaxKind::ARRAY_PATTERN_ELEMENT)
            .collect();
        assert_eq!(elements.len(), 3);

        let first_inner = elements[0]
            .children()
            .find(|n| n.kind() == SyntaxKind::PATTERN)
            .expect("first element should contain a PATTERN");
        assert_eq!(child_kinds(&first_inner), vec![SyntaxKind::BINDING_PATTERN]);

        let second_inner = elements[1]
            .children()
            .find(|n| n.kind() == SyntaxKind::PATTERN)
            .expect("second element should contain a PATTERN");
        assert_eq!(child_kinds(&second_inner), vec![SyntaxKind::TYPE_PATTERN]);

        assert!(
            elements[2].children_with_tokens().any(|c| {
                matches!(
                    c,
                    rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::DOT_DOT
                )
            }),
            "rest element should contain DOT_DOT"
        );
    }

    #[test]
    fn pattern_array_destructure_in_let_statement() {
        let source = r#"
function Demo() -> int {
  let [..let rest] = xs;
  1
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let pattern = first_let_pattern(&root);
        assert_eq!(child_kinds(&pattern), vec![SyntaxKind::ARRAY_PATTERN]);
    }

    #[test]
    fn pattern_array_rejects_multiple_rest_markers() {
        let source = r#"
function Demo() -> int {
  match (x) {
    [..let left, ..let right] => 1
  }
}
"#;

        let (_root, errors) = parse_source(source);
        assert!(
            !errors.is_empty(),
            "expected parse error for multiple array rest markers"
        );
    }

    #[test]
    fn pattern_match_bare_identifier_is_type_pattern() {
        // In match arms, bare `Foo` is a type/path pattern, NOT a binding.
        // Per the spec: "Type Expressions: `let` is explicitly not allowed,
        // always bare." Bindings always require `let` — so `Foo` must be a
        // TYPE_PATTERN, not a BINDING_PATTERN.
        let source = r#"
function Demo() -> int {
  match (x) {
    Foo => 1,
    _ => 0,
  }
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let arms: Vec<_> = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::MATCH_ARM)
            .collect();
        assert_eq!(arms.len(), 2);

        let first_pattern = arms[0]
            .children()
            .find(|n| n.kind() == SyntaxKind::PATTERN)
            .unwrap();
        assert_eq!(
            child_kinds(&first_pattern),
            vec![SyntaxKind::TYPE_PATTERN],
            "bare identifier in match arm must be a TYPE_PATTERN"
        );

        // And explicitly: NO BINDING_PATTERN should appear under the arm.
        assert!(
            arms[0]
                .descendants()
                .all(|n| n.kind() != SyntaxKind::BINDING_PATTERN),
            "bare identifier should NOT produce a BINDING_PATTERN"
        );
    }

    #[test]
    fn pattern_destructure_namespaced_class() {
        // `name.space.Class { ... }` — destructure on a dotted-path class
        // name. The path goes through parse_path inside the destructure,
        // which walks `WORD ('.' WORD)*` before the `{`.
        let source = r#"
function Demo() -> int {
  let name.space.Class { field } = obj;
  1
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let pattern = first_let_pattern(&root);
        let destructure = pattern
            .children()
            .find(|n| n.kind() == SyntaxKind::DESTRUCTURE_PATTERN)
            .expect("expected DESTRUCTURE_PATTERN");

        // The destructure should contain three WORD tokens for the path
        // segments and one or more DOT tokens.
        let words: Vec<String> = destructure
            .children_with_tokens()
            .filter_map(|c| match c {
                rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::WORD => {
                    Some(t.text().to_string())
                }
                _ => None,
            })
            .collect();
        // First three WORDs are path segments — `field` lives inside a
        // FIELD_PATTERN, not as a direct token of the destructure.
        assert_eq!(
            words,
            vec!["name".to_string(), "space".to_string(), "Class".to_string()],
            "destructure path tokens should be the dotted segments"
        );

        let dot_count = destructure
            .children_with_tokens()
            .filter(|c| {
                matches!(
                    c,
                    rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::DOT
                )
            })
            .count();
        assert_eq!(dot_count, 2, "expected 2 dots between 3 segments");

        // And the field is still recognised.
        let fields: Vec<_> = destructure
            .children()
            .filter(|n| n.kind() == SyntaxKind::FIELD_PATTERN)
            .collect();
        assert_eq!(fields.len(), 1);
        assert!(fields[0].text().to_string().contains("field"));
    }

    #[test]
    fn pattern_dotted_path_in_match_arm() {
        // Dotted path like `name.space.Enum.Variant` in a match arm. With no
        // `{` afterwards, this is a TYPE_PATTERN — the path tokens land
        // inside a TYPE_EXPR. Note: there's no separate "enum variant"
        // pattern kind; an enum variant is just a singleton type, so it
        // structurally IS a TYPE_PATTERN with a dotted path. (Same logic as
        // literals being types.)
        let source = r#"
function Demo() -> int {
  match (x) {
    name.space.Enum.Variant => 1,
    _ => 0,
  }
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let arm = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::MATCH_ARM)
            .expect("expected MATCH_ARM");
        let pattern = arm
            .children()
            .find(|n| n.kind() == SyntaxKind::PATTERN)
            .unwrap();
        assert_eq!(
            child_kinds(&pattern),
            vec![SyntaxKind::TYPE_PATTERN],
            "enum variant path must be a TYPE_PATTERN"
        );

        let type_pat = pattern
            .children()
            .find(|n| n.kind() == SyntaxKind::TYPE_PATTERN)
            .unwrap();

        // Path segments inside the TYPE_PATTERN: 4 WORDs joined by 3 DOTs.
        let words: Vec<String> = type_pat
            .descendants_with_tokens()
            .filter_map(|c| match c {
                rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::WORD => {
                    Some(t.text().to_string())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            words,
            vec![
                "name".to_string(),
                "space".to_string(),
                "Enum".to_string(),
                "Variant".to_string(),
            ],
        );
        let dot_count = type_pat
            .descendants_with_tokens()
            .filter(|c| {
                matches!(
                    c,
                    rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::DOT
                )
            })
            .count();
        assert_eq!(dot_count, 3);
    }

    #[test]
    fn pattern_dotted_paths_in_union() {
        // Multiple dotted paths in a UNION_PATTERN — exercises the union
        // loop not getting confused by paths. (Enum-variant-style input,
        // but structurally just three TYPE_PATTERN atoms.)
        let source = r#"
function Demo() -> int {
  match (x) {
    Status.Active | Status.Pending | other.pkg.Result.Ok => 1,
    _ => 0,
  }
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let arm = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::MATCH_ARM)
            .expect("expected MATCH_ARM");
        let pattern = arm
            .children()
            .find(|n| n.kind() == SyntaxKind::PATTERN)
            .unwrap();
        let union = pattern
            .children()
            .find(|n| n.kind() == SyntaxKind::UNION_PATTERN)
            .expect("expected UNION_PATTERN");

        let atoms: Vec<_> = union
            .children()
            .filter(|n| n.kind() == SyntaxKind::TYPE_PATTERN)
            .collect();
        assert_eq!(atoms.len(), 3, "expected three enum-variant atoms");
        assert!(atoms[0].text().to_string().contains("Status.Active"));
        assert!(atoms[1].text().to_string().contains("Status.Pending"));
        assert!(atoms[2].text().to_string().contains("other.pkg.Result.Ok"));
    }

    #[test]
    fn pattern_bare_underscore_is_wildcard() {
        // `_` in a match arm should be a WILDCARD_PATTERN, not a TYPE_PATTERN
        // with a WORD(_) inside. The new pattern parser distinguishes these
        // structurally so downstream code doesn't text-match `_`.
        let source = r#"
function Demo() -> int {
  match (x) {
    _ => 0,
  }
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let arm = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::MATCH_ARM)
            .expect("expected MATCH_ARM");
        let pattern = arm
            .children()
            .find(|n| n.kind() == SyntaxKind::PATTERN)
            .unwrap();
        assert_eq!(
            child_kinds(&pattern),
            vec![SyntaxKind::WILDCARD_PATTERN],
            "bare `_` must be a WILDCARD_PATTERN"
        );

        // No TYPE_PATTERN should appear under the arm.
        assert!(
            arm.descendants()
                .all(|n| n.kind() != SyntaxKind::TYPE_PATTERN),
            "bare `_` should NOT produce a TYPE_PATTERN"
        );
    }

    #[test]
    fn pattern_let_underscore_is_wildcard() {
        // `let _ = ...` — the `let _` form is also a WILDCARD_PATTERN, NOT a
        // BINDING_PATTERN to a name called `_`. The wildcard node contains
        // both the `let` keyword and the `_` token.
        let source = r#"
function Demo() -> int {
  let _ = 1;
  1
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let pattern = first_let_pattern(&root);
        assert_eq!(
            child_kinds(&pattern),
            vec![SyntaxKind::WILDCARD_PATTERN],
            "`let _` must be a WILDCARD_PATTERN, not a BINDING_PATTERN"
        );

        let wildcard = pattern
            .children()
            .find(|n| n.kind() == SyntaxKind::WILDCARD_PATTERN)
            .unwrap();
        let has_let = wildcard.children_with_tokens().any(|c| {
            matches!(
                c,
                rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::KW_LET
            )
        });
        assert!(has_let, "`let _` wildcard must keep the KW_LET token");
    }

    #[test]
    fn pattern_negative_integer_literal_in_match_arm() {
        // `-42` as a match-arm pattern. Structurally a TYPE_PATTERN whose
        // inner TYPE_EXPR carries a MINUS token followed by an INTEGER_LITERAL.
        let source = r#"
function Demo() -> int {
  match (x) {
    -42 => 1,
    _ => 0,
  }
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let arm = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::MATCH_ARM)
            .unwrap();
        let pattern = arm
            .children()
            .find(|n| n.kind() == SyntaxKind::PATTERN)
            .unwrap();
        let type_pat = pattern
            .children()
            .find(|n| n.kind() == SyntaxKind::TYPE_PATTERN)
            .expect("expected TYPE_PATTERN for `-42`");
        assert!(type_pat.text().to_string().contains("-42"));

        // The structural shape should be MINUS then INTEGER_LITERAL.
        let kinds: Vec<SyntaxKind> = type_pat
            .descendants_with_tokens()
            .filter_map(|c| match c {
                rowan::NodeOrToken::Token(t)
                    if matches!(t.kind(), SyntaxKind::MINUS | SyntaxKind::INTEGER_LITERAL) =>
                {
                    Some(t.kind())
                }
                _ => None,
            })
            .collect();
        assert_eq!(kinds, vec![SyntaxKind::MINUS, SyntaxKind::INTEGER_LITERAL]);
    }

    #[test]
    fn generic_args_with_negative_integer_literal() {
        // `Factory<-1>(x)` — `<-1>` should disambiguate as GENERIC_ARGS, not as
        // a comparison with a unary-minus literal. Without `Minus` in
        // `looks_like_generic_args`'s allow-list, the parser falls back to
        // comparison parsing and `(x)` becomes detached.
        let source = r#"
function Demo() -> int {
  let y = Factory<-1>(x);
  1
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let generic_args = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::GENERIC_ARGS)
            .expect("expected GENERIC_ARGS for `Factory<-1>(x)`");
        let text = generic_args.text().to_string();
        assert!(
            text.contains("-1"),
            "expected `-1` inside GENERIC_ARGS, got `{text}`"
        );
    }

    #[test]
    fn class_field_negative_integer_literal_type_with_colon() {
        // `field: -1` — `is_at_type_start` must accept `Minus` followed by an
        // integer literal so the class-field gate doesn't reject negative
        // literal types and fall through to "missing a type annotation".
        let source = r#"
class Foo {
  literal_neg_one: -1
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let field = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::FIELD)
            .expect("expected FIELD node");
        let type_expr = field
            .descendants()
            .find(|n| n.kind() == SyntaxKind::TYPE_EXPR)
            .expect("expected TYPE_EXPR inside FIELD");
        assert!(
            type_expr.text().to_string().contains("-1"),
            "expected field type to contain `-1`, got: {}",
            type_expr.text()
        );
    }

    #[test]
    fn class_field_negative_integer_literal_type_no_colon() {
        // BEP-019 colon-less form: `field -1`. Same gate, exercises the path
        // where `has_colon == false` so the type must start on the same line.
        let source = r#"
class Foo {
  literal_neg_one -1
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let field = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::FIELD)
            .expect("expected FIELD node");
        let type_expr = field
            .descendants()
            .find(|n| n.kind() == SyntaxKind::TYPE_EXPR)
            .expect("expected TYPE_EXPR inside FIELD");
        assert!(
            type_expr.text().to_string().contains("-1"),
            "expected field type to contain `-1`, got: {}",
            type_expr.text()
        );
    }

    #[test]
    fn class_field_negative_literal_in_union_type() {
        // `-1 | 0 | 1` in field position — combines the field-start gate with
        // the union-continuation path inside `parse_type_with`.
        let source = r#"
class Foo {
  bounded: -1 | 0 | 1
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let field = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::FIELD)
            .expect("expected FIELD node");
        let type_expr = field
            .descendants()
            .find(|n| n.kind() == SyntaxKind::TYPE_EXPR)
            .expect("expected TYPE_EXPR inside FIELD");
        let text = type_expr.text().to_string();
        assert!(
            text.contains("-1") && text.contains('0') && text.contains('1'),
            "expected union to contain -1, 0, 1; got: {text}"
        );
    }

    #[test]
    fn function_parameter_negative_integer_literal_type() {
        // `(x: -1) -> int` — same gate is consulted by `parse_parameter`.
        let source = r#"
function Demo(x: -1) -> int {
  0
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let param = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::PARAMETER)
            .expect("expected PARAMETER node");
        let type_expr = param
            .descendants()
            .find(|n| n.kind() == SyntaxKind::TYPE_EXPR)
            .expect("expected TYPE_EXPR inside PARAMETER");
        assert!(
            type_expr.text().to_string().contains("-1"),
            "expected parameter type to contain `-1`, got: {}",
            type_expr.text()
        );
    }

    // ============ for-in: `let` is required ============
    //
    // `for ... in ...` always requires a `let`-prefixed pattern; the variable
    // binding lives inside `LET_STMT > PATTERN`. Bare-WORD forms (`for (i in
    // xs)` or `for i in xs`) are rejected.

    /// Find the `FOR_EXPR`'s iterator-style binding: a `LET_STMT` child of `FOR_EXPR`
    /// containing a single PATTERN child.
    fn first_for_in_pattern(root: &SyntaxNode) -> SyntaxNode {
        let for_expr = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::FOR_EXPR)
            .expect("expected FOR_EXPR");
        let let_stmt = for_expr
            .children()
            .find(|n| n.kind() == SyntaxKind::LET_STMT)
            .expect("for-in should expose its binding as a LET_STMT child");
        let_stmt
            .children()
            .find(|n| n.kind() == SyntaxKind::PATTERN)
            .expect("LET_STMT under for-in should contain a PATTERN")
    }

    #[test]
    fn for_in_paren_with_let_binding() {
        let source = r#"
function Demo() -> int {
  for (let i in xs) {
    1
  }
  1
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let pattern = first_for_in_pattern(&root);
        assert_eq!(
            child_kinds(&pattern),
            vec![SyntaxKind::BINDING_PATTERN],
            "for (let i in xs) should produce a BINDING_PATTERN inside PATTERN"
        );
    }

    #[test]
    fn for_in_no_paren_with_let_binding() {
        // The non-parenthesized form still requires `let`.
        let source = r#"
function Demo() -> int {
  for let i in xs {
    1
  }
  1
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let pattern = first_for_in_pattern(&root);
        assert_eq!(child_kinds(&pattern), vec![SyntaxKind::BINDING_PATTERN],);
    }

    #[test]
    fn for_in_with_wildcard() {
        let source = r#"
function Demo() -> int {
  for (let _ in xs) {
    1
  }
  1
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let pattern = first_for_in_pattern(&root);
        assert_eq!(
            child_kinds(&pattern),
            vec![SyntaxKind::WILDCARD_PATTERN],
            "for (let _ in xs) should produce a WILDCARD_PATTERN, not a BINDING"
        );
    }

    #[test]
    fn for_in_with_destructure_pattern() {
        // The for-in binding accepts arbitrary patterns — including
        // destructures — so long as `let` is present.
        let source = r#"
function Demo() -> int {
  for (let Pair { a, b } in xs) {
    1
  }
  1
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let pattern = first_for_in_pattern(&root);
        assert_eq!(child_kinds(&pattern), vec![SyntaxKind::DESTRUCTURE_PATTERN],);
    }

    #[test]
    fn for_in_paren_without_let_is_rejected() {
        // `for (i in xs)` — bare WORD inside parens. Used to be a valid
        // iterator form; with the unified pattern model, bindings always
        // require `let`, so this now lands on the C-style path which
        // immediately mis-parses, producing diagnostics.
        let source = r#"
function Demo() -> int {
  for (i in xs) {
    1
  }
  1
}
"#;
        let (_root, errors) = parse_source(source);
        assert!(
            !errors.is_empty(),
            "expected parse errors for let-less for-in form, got none"
        );
    }

    #[test]
    fn for_in_no_paren_without_let_is_rejected() {
        // `for i in xs` (non-parenthesized) used to also be valid as a
        // bare-WORD iterator form. Now it requires `let` — the parser sees
        // the bare WORD where it expects `let`, errors, and produces
        // diagnostics.
        let source = r#"
function Demo() -> int {
  for i in xs {
    1
  }
  1
}
"#;
        let (_root, errors) = parse_source(source);
        assert!(
            !errors.is_empty(),
            "expected parse errors for let-less non-paren for-in, got none"
        );
    }

    #[test]
    fn for_c_style_still_works() {
        // C-style for loops are unaffected by the iterator-form changes.
        let source = r#"
function Demo() -> int {
  for (let i = 0; i < 10; i = i + 1) {
    1
  }
  1
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let for_expr = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::FOR_EXPR)
            .expect("expected FOR_EXPR");
        // C-style has no `KW_IN` — distinguishes it from the iterator form.
        let has_in = for_expr.children_with_tokens().any(|c| {
            matches!(
                c,
                rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::KW_IN
            )
        });
        assert!(!has_in, "C-style for must not contain KW_IN");
    }

    #[test]
    fn function_parameter_defaults_parse() {
        let source = r#"
function Search(query: string, max_results: int = 10, filter: string = "none") -> int {
  1
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let params: Vec<_> = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::PARAMETER)
            .collect();
        assert_eq!(params.len(), 3);
        assert!(
            params[1].children_with_tokens().any(
                |it| matches!(it, rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::EQUALS)
            ),
            "expected second parameter to preserve default equals token"
        );
        assert!(
            params[2].children_with_tokens().any(
                |it| matches!(it, rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::EQUALS)
            ),
            "expected third parameter to preserve default equals token"
        );
    }

    #[test]
    fn self_parameter_default_recovery_parse() {
        let source = r#"
class Response {
  function text(self = default_self()) -> string {
    "ok"
  }
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let self_param = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::PARAMETER && n.text().to_string().contains("self"))
            .expect("expected self PARAMETER node");
        assert!(
            self_param.children_with_tokens().any(
                |it| matches!(it, rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::EQUALS)
            ),
            "expected self parameter recovery to preserve default equals token"
        );
    }

    #[test]
    fn lambda_parameter_defaults_parse_for_recovery() {
        let source = r#"
function Demo() -> int {
  let f = (x: int = 1) -> int {
    x
  }
  f()
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let lambda_param = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::LAMBDA_EXPR)
            .and_then(|lambda| {
                lambda
                    .descendants()
                    .find(|n| n.kind() == SyntaxKind::PARAMETER)
            })
            .expect("expected lambda PARAMETER node");
        assert!(
            lambda_param.children_with_tokens().any(
                |it| matches!(it, rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::EQUALS)
            ),
            "expected lambda parameter default equals token"
        );
    }

    #[test]
    fn function_type_parameter_default_reports_error_and_recovers() {
        let source = r#"
type Searcher = (query: string = make_default("cats"), limit?: int) -> int
"#;
        let (root, errors) = parse_source(source);

        let default_error_span = errors.iter().find_map(|error| match error {
            ParseError::InvalidSyntax { message, span }
                if message == "default expressions are not allowed in function types" =>
            {
                Some(span.range)
            }
            _ => None,
        });
        assert!(
            default_error_span.is_some(),
            "expected function type default diagnostic, got: {errors:#?}"
        );
        let default_error_span = default_error_span.unwrap();
        let default_start = source.find("make_default").unwrap();
        let default_end = source.find(", limit").unwrap();
        assert_eq!(
            u32::from(default_error_span.start()) as usize,
            default_start
        );
        assert_eq!(u32::from(default_error_span.end()) as usize, default_end);
        assert!(
            root.descendants()
                .any(|n| n.kind() == SyntaxKind::TYPE_ALIAS_DEF),
            "expected parser to recover through the type alias"
        );
        assert_eq!(
            root.descendants()
                .filter(|n| n.kind() == SyntaxKind::FUNCTION_TYPE_PARAM)
                .count(),
            2,
            "expected parser to recover and keep both function type params"
        );
    }

    #[test]
    fn named_call_arguments_parse_as_call_args() {
        let source = r#"
function Demo() -> int {
  Search(query = "cats", max_results = 5)
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let call_args: Vec<_> = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::CALL_ARG)
            .collect();
        assert_eq!(call_args.len(), 2);
        assert!(call_args.iter().all(|arg| arg.children_with_tokens().any(
            |it| matches!(it, rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::EQUALS)
        )));
    }

    #[test]
    fn function_type_optional_params_parse() {
        let source = r#"
type Searcher = (query: string, max_results?: int) -> int
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let optional_param = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::FUNCTION_TYPE_PARAM)
            .nth(1)
            .expect("expected second function type parameter");
        assert!(optional_param.children_with_tokens().any(
            |it| matches!(it, rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::QUESTION)
        ));
    }
}
