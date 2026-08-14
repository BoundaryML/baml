//! Parser implementation.
//!
//! Implements a recursive descent parser with error recovery.

use baml_base::Span;
use baml_compiler_lexer::{Token, TokenKind};
use baml_compiler_syntax::SyntaxKind;
use rowan::{GreenNode, GreenNodeBuilder, NodeCache, TextSize};
use text_size::TextRange;

use crate::ParseError;

/// The `if let` form quoted by the postfix-`!` diagnostic, with `T` and `opt`
/// standing in for the user's own type and optional value.
///
/// Kept as a constant so the `optional_unwrap_hint_suggests_real_syntax` test
/// can instantiate those metavariables and assert the suggested form actually
/// parses. A hint that doesn't parse drives users into a second error cascade
/// instead of fixing the first.
const IF_LET_UNWRAP_SHAPE: &str = "if let x: T = opt { ... }";

pub fn parse_file(tokens: &[Token]) -> (GreenNode, Vec<ParseError>) {
    parse_impl(tokens, None)
}

/// Map lexer token kinds to syntax kinds.
fn token_kind_to_syntax_kind(kind: TokenKind) -> SyntaxKind {
    match kind {
        // Keywords
        TokenKind::Class => SyntaxKind::KW_CLASS,
        TokenKind::Enum => SyntaxKind::KW_ENUM,
        TokenKind::Interface => SyntaxKind::KW_INTERFACE,
        TokenKind::Implements => SyntaxKind::KW_IMPLEMENTS,
        TokenKind::Implement => SyntaxKind::KW_IMPLEMENT,
        TokenKind::Extends => SyntaxKind::KW_EXTENDS,
        TokenKind::Requires => SyntaxKind::KW_REQUIRES,
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
        TokenKind::Instanceof => SyntaxKind::KW_INSTANCEOF,
        TokenKind::Is => SyntaxKind::KW_IS,
        TokenKind::Dynamic => SyntaxKind::KW_DYNAMIC,
        TokenKind::Match => SyntaxKind::KW_MATCH,
        TokenKind::Catch => SyntaxKind::KW_CATCH,
        TokenKind::CatchAll => SyntaxKind::KW_CATCH_ALL,
        TokenKind::Throws => SyntaxKind::KW_THROWS,
        TokenKind::Spawn => SyntaxKind::KW_SPAWN,
        TokenKind::Await => SyntaxKind::KW_AWAIT,
        TokenKind::Defer => SyntaxKind::KW_DEFER,

        // Literals
        TokenKind::Word => SyntaxKind::WORD,
        TokenKind::Quote => SyntaxKind::QUOTE,
        TokenKind::Hash => SyntaxKind::HASH,
        TokenKind::Backtick => SyntaxKind::BACKTICK,
        TokenKind::BigintLiteral => SyntaxKind::BIGINT_LITERAL,
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

/// BEP-049 §5 first-token dispatch result for `${...}` interpolation
/// content. Used by `classify_backtick_interp` to decide which parser
/// path to take without committing tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BacktickInterpForm {
    /// `${for (...)}` — opens a `for`-loop block-tag. Body lives in the
    /// surrounding backtick content until a matching `${endfor}`.
    For,
    /// `${endfor}` — closes the innermost open `${for}`.
    Endfor,
    /// `${if (...)}` — opens an `if` block-tag. Distinguished from an
    /// if-expression form by post-condition lookahead: `}` closes the
    /// interp (block-tag), `{` opens a then-block (expression).
    IfBlockTag,
    /// `${else if (...)}` — continuation of an open `${if}` block-tag.
    ElseIfBlockTag,
    /// `${else}` — continuation of an open `${if}` block-tag.
    ElseBlockTag,
    /// `${endif}` — closes the innermost open `${if}`.
    Endif,
    /// Anything else — falls through to the M2 block-expression path.
    Expression,
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
    /// While parsing an unparenthesized for-in iterable, permit an object
    /// literal only when its closing brace is immediately followed by syntax
    /// that continues the iterable (or by the loop body's opening brace).
    ///
    /// This keeps `for let x in xs { body }` unambiguous while allowing
    /// iterables such as `Values { items }.items`.
    allow_object_literal_before_for_body_depth: u32,
    /// Suppresses destructure patterns (`Class { fields }`) in pattern
    /// position, mirroring Rust's `Restrictions::NO_STRUCT_LITERAL` for
    /// struct literals in `if` / `while` condition expressions. Bumped
    /// around `parse_expr` calls in those condition positions; reset
    /// (`mem::take` + restore) when entering a `PAREN_PATTERN` or
    /// `ARRAY_PATTERN` so nested patterns regain normal destructure
    /// parsing. Counter so nested conditions nest correctly.
    suppress_destructure_pattern_depth: u32,
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
            allow_object_literal_before_for_body_depth: 0,
            suppress_destructure_pattern_depth: 0,
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

    fn token_is_contextual_kw(token: Option<&Token>, kw: &str) -> bool {
        token
            .map(|t| t.kind == TokenKind::Word && t.text == kw)
            .unwrap_or(false)
    }

    /// Check if the current token is a `Word` with the given text.
    /// Used for contextual keywords like `with` and binding-position `const`
    /// that should not be reserved globally.
    fn at_contextual_kw(&self, kw: &str) -> bool {
        Self::token_is_contextual_kw(self.current(), kw)
    }

    fn peek_is_contextual_kw(&self, n: usize, kw: &str) -> bool {
        Self::token_is_contextual_kw(self.peek(n), kw)
    }

    /// Consume the current contextual keyword token, re-labelling it as
    /// `syntax_kind` in the syntax tree. Handles leading trivia just like
    /// [`Self::bump`].
    fn bump_contextual_kw_as(&mut self, kw: &str, syntax_kind: SyntaxKind) {
        debug_assert!(self.at_contextual_kw(kw));
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
        // Emit the contextual Word token as the requested keyword kind.
        if self.current < self.tokens.len() {
            self.events.push(Event::Token {
                kind: syntax_kind,
                text: self.tokens[self.current].text.clone(),
            });
            self.current += 1;
        }
    }

    /// Consume the current `Word("with")` token, re-labelling it as `KW_WITH`
    /// in the syntax tree.
    fn bump_contextual_with(&mut self) {
        self.bump_contextual_kw_as("with", SyntaxKind::KW_WITH);
    }

    fn binding_intro_follower(kind: Option<TokenKind>) -> bool {
        matches!(kind, Some(TokenKind::Word | TokenKind::LBracket))
    }

    /// True at either `let` or contextual `const`.
    fn at_binding_intro(&self) -> bool {
        self.at(TokenKind::Let) || self.at_contextual_kw("const")
    }

    /// True for positions that dispatch to a binding statement or
    /// statement-like binding head. `let` stays a real keyword; contextual
    /// `const` only claims the position when a pattern-shaped token follows.
    fn at_binding_intro_stmt(&self) -> bool {
        self.at(TokenKind::Let)
            || (self.at_contextual_kw("const")
                && Self::binding_intro_follower(self.peek(1).map(|t| t.kind)))
    }

    fn at_binding_intro_pattern(&self) -> bool {
        self.at(TokenKind::Let)
            || (self.at_contextual_kw("const")
                && Self::binding_intro_follower(self.peek(1).map(|t| t.kind)))
    }

    fn peek_is_binding_intro(&self, n: usize) -> bool {
        self.peek(n).map(|t| t.kind) == Some(TokenKind::Let)
            || self.peek_is_contextual_kw(n, "const")
    }

    fn peek_is_binding_intro_stmt(&self, n: usize) -> bool {
        self.peek(n).map(|t| t.kind) == Some(TokenKind::Let)
            || (self.peek_is_binding_intro(n)
                && Self::binding_intro_follower(self.peek(n + 1).map(|t| t.kind)))
    }

    fn binding_intro_is_followed_by_array_pattern(&self) -> bool {
        self.peek(1).map(|t| t.kind) == Some(TokenKind::LBracket)
    }

    fn bump_binding_intro(&mut self) {
        if self.at(TokenKind::Let) {
            self.bump();
        } else if self.at_contextual_kw("const") {
            self.bump_contextual_kw_as("const", SyntaxKind::KW_CONST);
        } else {
            self.error_unexpected_token("'let' or 'const'".to_string());
        }
    }

    /// Check if the current token can start a type expression.
    /// Valid type starts: Word (type name), string literal, integer/float literal,
    /// `-` followed by an integer/float literal (negative literal type), `LParen` (tuple).
    fn is_at_type_start(&self) -> bool {
        self.at(TokenKind::Word)
            || self.at(TokenKind::Quote) // string literal type
            || self.at(TokenKind::Hash) // raw string literal type
            || self.at(TokenKind::BigintLiteral)
            || self.at(TokenKind::IntegerLiteral)
            || self.at(TokenKind::FloatLiteral)
            || self.at(TokenKind::LParen) // tuple/parenthesized type
            || self.at(TokenKind::Less) // generic function type: <T>(T) -> U
            || (self.at(TokenKind::Minus)
                && matches!(
                    self.peek(1).map(|t| t.kind),
                    Some(
                        TokenKind::BigintLiteral
                            | TokenKind::IntegerLiteral
                            | TokenKind::FloatLiteral
                    )
                ))
    }

    /// Check if a token kind is basic trivia (whitespace/newlines, not comments).
    /// Comments are also conceptually trivia, but they're assembled from token patterns (// and /*).
    #[allow(clippy::unused_self)]
    fn is_basic_trivia(&self, kind: TokenKind) -> bool {
        matches!(kind, TokenKind::Whitespace | TokenKind::Newline)
    }

    /// True when the current token can serve as a member name after `.`.
    ///
    /// `interface`/`implements`/`extends` are keywords for declarations but
    /// remain valid as member names — e.g. `dog_t.implements(animal_t)` on the
    /// reflection `type` value.
    fn at_member_name(&self) -> bool {
        self.at(TokenKind::Word)
            || self.at(TokenKind::Implements)
            || self.at(TokenKind::Implement)
            || self.at(TokenKind::Extends)
            || self.at(TokenKind::Requires)
            || self.at(TokenKind::Interface)
            // `client` is a keyword for LLM config/declarations, but stays valid
            // as a class field name and member-access name so BEP-049 §10's
            // `ctx.client` (on the `Context` type) parses. Unambiguous here:
            // class bodies and `.member` access have no `client` construct.
            || self.at(TokenKind::Client)
    }

    /// True for `field as class_field` inside an `implements` block.
    fn looks_like_interface_field_link(&self) -> bool {
        let first = self.skip_trivia_and_comments_from(self.current);
        let Some(first_token) = self.tokens.get(first) else {
            return false;
        };
        if !matches!(
            first_token.kind,
            TokenKind::Word
                | TokenKind::Implements
                | TokenKind::Implement
                | TokenKind::Extends
                | TokenKind::Requires
                | TokenKind::Interface
        ) {
            return false;
        }
        let second = self.skip_trivia_and_comments_from(first + 1);
        self.tokens
            .get(second)
            .is_some_and(|t| t.kind == TokenKind::Word && t.text == "as")
    }

    /// True for the BEP-044 projection operator `.as<T>`.
    fn looks_like_as_projection(&self) -> bool {
        let dot = self.skip_trivia_and_comments_from(self.current);
        if self
            .tokens
            .get(dot)
            .is_none_or(|t| t.kind != TokenKind::Dot)
        {
            return false;
        }
        let as_idx = self.skip_trivia_and_comments_from(dot + 1);
        if self
            .tokens
            .get(as_idx)
            .is_none_or(|t| !(t.kind == TokenKind::Word && t.text == "as"))
        {
            return false;
        }
        let less_idx = self.skip_trivia_and_comments_from(as_idx + 1);
        self.tokens
            .get(less_idx)
            .is_some_and(|t| t.kind == TokenKind::Less)
    }

    /// True for a type-level associated type projection: `(T as I).Item`.
    fn looks_like_associated_type_projection(&self) -> bool {
        let mut i = self.skip_trivia_and_comments_from(self.current);
        if self
            .tokens
            .get(i)
            .is_none_or(|t| t.kind != TokenKind::LParen)
        {
            return false;
        }
        i += 1;

        let mut paren_depth = 1_i32;
        let mut angle_depth = 0_i32;
        let mut saw_as = false;
        let mut rparen_idx = None;

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
                TokenKind::LParen => paren_depth += 1,
                TokenKind::RParen => {
                    if paren_depth == 1 && angle_depth == 0 {
                        rparen_idx = Some(i);
                        break;
                    }
                    paren_depth -= 1;
                }
                TokenKind::Less => angle_depth += 1,
                TokenKind::Greater => angle_depth -= 1,
                TokenKind::GreaterGreater => angle_depth -= 2,
                TokenKind::Word if token.text == "as" && paren_depth == 1 && angle_depth == 0 => {
                    saw_as = true;
                }
                _ => {}
            }
            i += 1;
        }

        if !saw_as {
            return false;
        }
        let Some(rparen_idx) = rparen_idx else {
            return false;
        };
        let dot_idx = self.skip_trivia_and_comments_from(rparen_idx + 1);
        if self
            .tokens
            .get(dot_idx)
            .is_none_or(|t| t.kind != TokenKind::Dot)
        {
            return false;
        }
        let member_idx = self.skip_trivia_and_comments_from(dot_idx + 1);
        self.tokens
            .get(member_idx)
            .is_some_and(|t| t.kind == TokenKind::Word)
    }

    /// True when a postfix `(` must NOT be glued onto the expression parsed so
    /// far as a call, because that expression is *block-terminated* (its last
    /// real token is `}` — an `if`/`match`/block/loop body) and the `(` opens a
    /// fresh line.
    ///
    /// This is the guard-`if` early-return case (B-622):
    ///   if (x < 0) { throw ... }
    ///   (x * 2)
    /// Without it the parser glues `{ ... }(x * 2)` into a call on the void
    /// `if` result. Keying on a block-terminating `}` (rather than any callee)
    /// keeps ordinary multi-line calls — a method chain whose `(` lands on a
    /// later line than an identifier/`)` callee — parsing as calls, matching
    /// the deliberately-tested `foo()`-then-`(1)` chained-call behavior. The
    /// newline requirement keeps same-line `<block>(…)` forms (e.g. an
    /// immediately-invoked block) untouched.
    fn newline_separates_block_expr_from_paren(&self) -> bool {
        self.has_newline_ahead() && self.last_significant_emitted_token_is_block_close()
    }

    /// Kind-check on the most recently *emitted* significant token: is it a
    /// closing `}`? Trailing trivia after the current expression has not been
    /// emitted yet at postfix-decision time, so the last non-trivia token event
    /// is the final real token of the expression parsed so far.
    fn last_significant_emitted_token_is_block_close(&self) -> bool {
        self.events.iter().rev().find_map(|event| match event {
            Event::Token { kind, .. } if !kind.is_trivia() => Some(*kind),
            _ => None,
        }) == Some(SyntaxKind::R_BRACE)
    }

    /// Check if there's a newline before the next non-trivia token.
    /// Comments are treated as trivia for this purpose.
    fn has_newline_ahead(&self) -> bool {
        let mut i = self.current;
        while i < self.tokens.len() {
            // Skip comments (they're trivia for line-termination purposes) — but
            // a block comment can itself span lines (`/*\n*/`), and that interior
            // newline still terminates the line, so a tag separated from its
            // backtick by such a comment must NOT absorb it as a tagged template.
            let new_i = self.skip_comment_at(i);
            if new_i != i {
                if self.tokens[i..new_i]
                    .iter()
                    .any(|t| t.kind == TokenKind::Newline)
                {
                    return true;
                }
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

    /// Check if position i starts a line comment (`//`), including `//#`.
    ///
    /// Header comments are ordinary trivia for navigation. Expression-context loops inspect the
    /// raw token stream with `at_header_comment_start` before normal trivia consumption to preserve
    /// them as `HEADER_COMMENT` nodes; ordinary line-comment consumption reports them as invalid.
    fn is_line_comment_at(&self, i: usize) -> bool {
        if i + 1 < self.tokens.len()
            && self.tokens[i].kind == TokenKind::Slash
            && self.tokens[i + 1].kind == TokenKind::Slash
        {
            return true;
        }
        // A `#!` shebang is treated just like `//`, so .baml files can be shebang'd
        //   #!/usr/bin/env -S baml run --file
        if i + 1 < self.tokens.len()
            && self.tokens[i].kind == TokenKind::Hash
            && self.tokens[i + 1].kind == TokenKind::Not
        {
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
            } else if self.is_header_comment_at(i) {
                return true;
            } else if self.is_line_comment_at(i) {
                // Skip an ordinary line comment.
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
        false
    }

    /// Check if we're at the start of a block comment (/*)
    fn at_block_comment_start(&self) -> bool {
        self.is_block_comment_at(self.current)
    }

    /// Consume a line comment (`//`) as a single `LINE_COMMENT` token.
    ///
    /// A header normally produces a diagnostic here. The one trivia-only exception is a header
    /// immediately before an expression function, which is consumed through
    /// [`Self::consume_function_header_comment`] instead.
    fn consume_line_comment(&mut self) {
        self.consume_line_comment_impl(true);
    }

    fn consume_line_comment_impl(&mut self, diagnose_header: bool) {
        let is_header = self.is_header_comment_at(self.current);
        let comment_start = self.current;

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

        if diagnose_header && is_header {
            let start = self.tokens[comment_start].span;
            let end = self.tokens[self.current - 1].span;
            self.error(
                "header comments (`//#`) are only allowed in expression functions".to_string(),
                Self::span_from_to(start, end),
            );
        }

        // Emit as a single token (not wrapped in a node)
        self.events.push(Event::Token {
            kind: SyntaxKind::LINE_COMMENT,
            text,
        });
    }

    /// Consume a `//#` immediately preceding an expression function as line-comment trivia.
    ///
    /// Function-level headers are read from source text by the visualization layer; unlike headers
    /// inside expression bodies, they must not become `HEADER_COMMENT` statement nodes.
    fn consume_function_header_comment(&mut self) {
        while self.current < self.tokens.len() {
            let kind = self.tokens[self.current].kind;
            if kind == TokenKind::Whitespace || kind == TokenKind::Newline {
                self.events.push(Event::Token {
                    kind: token_kind_to_syntax_kind(kind),
                    text: self.tokens[self.current].text.clone(),
                });
                self.current += 1;
            } else if self.is_header_comment_at(self.current) {
                self.consume_line_comment_impl(false);
                break;
            } else if self.is_line_comment_at(self.current) {
                self.consume_line_comment();
            } else if self.is_block_comment_at(self.current) {
                self.consume_block_comment();
            } else {
                break;
            }
        }
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
            } else if self.is_header_comment_at(self.current) {
                break;
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
                    | TokenKind::Interface
                    | TokenKind::Function
                    | TokenKind::Implements
                    | TokenKind::Implement
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

    /// Distinguishes a top-level `client<llm> Name { … }` declaration (which,
    /// inside a class body, signals a missing `}` to recover from) from a class
    /// field named `client` (BEP-049 §10 `ctx.client`). The declaration form is
    /// `client<…>`; a field is `client Type` / `client:`.
    fn looks_like_client_declaration_start(&self) -> bool {
        let current = self.skip_trivia_and_comments_from(self.current);
        if self
            .tokens
            .get(current)
            .is_none_or(|t| t.kind != TokenKind::Client)
        {
            return false;
        }
        let next = self.skip_trivia_and_comments_from(current + 1);
        self.tokens
            .get(next)
            .is_some_and(|t| t.kind == TokenKind::Less)
    }

    fn looks_like_interface_declaration_start(&self) -> bool {
        let current = self.skip_trivia_and_comments_from(self.current);
        if self
            .tokens
            .get(current)
            .is_none_or(|t| t.kind != TokenKind::Interface)
        {
            return false;
        }

        let name = self.skip_trivia_and_comments_from(current + 1);
        if self
            .tokens
            .get(name)
            .is_none_or(|t| t.kind != TokenKind::Word)
        {
            return false;
        }

        let mut i = name + 1;
        while i < self.tokens.len() {
            let new_i = self.skip_comment_at(i);
            if new_i != i {
                i = new_i;
                continue;
            }
            let token = &self.tokens[i];
            match token.kind {
                TokenKind::Whitespace | TokenKind::Newline => {
                    i += 1;
                }
                TokenKind::LBrace => return true,
                TokenKind::RBrace | TokenKind::Semicolon => return false,
                _ => {
                    i += 1;
                }
            }
        }
        false
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
            || self.at_binding_intro_stmt()
            || matches!(
                self.current().map(|t| t.kind),
                Some(
                    TokenKind::Return
                        | TokenKind::While
                        | TokenKind::For
                        | TokenKind::Break
                        | TokenKind::Continue
                        | TokenKind::Throw
                        | TokenKind::Defer
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
                "unknown keyword `{invalid_keyword}`; expected `class`, `enum`, `function`, `client`, `generator`, `test`, or `type`"
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
                "unknown keyword `{invalid_keyword}`; did you mean `type`? usage: `type Name = expression`"
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

    /// Consume exactly the token at `self.current` and emit it, regardless
    /// of whether it's basic trivia. Unlike `bump_raw`, does NOT first walk
    /// past whitespace/newlines. Used for the second half of a `\\<char>`
    /// escape in backtick content, where the next single raw token IS the
    /// escape target and must not be conflated with leading trivia.
    fn bump_one_token_raw(&mut self) {
        if let Some(token) = self.tokens.get(self.current) {
            let kind = token_kind_to_syntax_kind(token.kind);
            self.events.push(Event::Token {
                kind,
                text: token.text.clone(),
            });
            self.current += 1;
        }
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

    /// The span to attach to an "expected …, found …" / point-at-here error.
    ///
    /// Points at the current token — `current()` already skips trivia, so its
    /// span is tight. At end of input there is no token to point at (the error
    /// is the *absence* of a token), so we emit a zero-width caret just past the
    /// last real token, matching rustc's `prev_token.span.shrink_to_hi()`. This
    /// never points at trailing trivia: the old fallback reached into the raw
    /// `self.tokens.last()`, which could be a trailing newline and produced a
    /// span covering `"\n"`.
    fn error_span(&self) -> baml_base::Span {
        if let Some(token) = self.current() {
            return token.span;
        }
        match self.last_non_trivia_token() {
            Some(token) => {
                let end = token.span.range.end();
                baml_base::Span::new(token.span.file_id, TextRange::new(end, end))
            }
            None => baml_base::Span::new(baml_base::FileId::new(0), TextRange::default()),
        }
    }

    /// The last non-trivia token in the stream, used to anchor end-of-input
    /// error spans so they never land on trailing whitespace or comments.
    ///
    /// Comments are token sequences (e.g. `// …`), not single trivia tokens, so
    /// a reverse scan can't recognize them; mirror `current_impl` and walk
    /// forward, skipping comment runs with `skip_comment_at`, tracking the last
    /// real (non-basic-trivia) token.
    fn last_non_trivia_token(&self) -> Option<&Token> {
        let mut last = None;
        let mut i = 0;
        while i < self.tokens.len() {
            let new_i = self.skip_comment_at(i);
            if new_i != i {
                i = new_i;
                continue;
            }
            let token = &self.tokens[i];
            if !self.is_basic_trivia(token.kind) {
                last = Some(token);
            }
            i += 1;
        }
        last
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

            let span = self.error_span();

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

        let span = self.error_span();

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

    /// Emit a hard syntax error (custom message) at the current token's span.
    fn error_here(&mut self, message: String) {
        let span = self.error_span();
        self.error(message, span);
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
                    // Found all hashes, now skip basic trivia to find next token.
                    while i < self.tokens.len() && self.is_basic_trivia(self.tokens[i].kind) {
                        i += 1;
                    }
                    // `None` if the hashes run to EOF with no token after them,
                    // so callers never index past the end on incomplete input.
                    return (i < self.tokens.len()).then_some(i);
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
        // `find_token_after_hashes` can point at the EOF slot (== tokens.len())
        // on incomplete input, so index through `get` rather than `[]`.
        if quote_pos.and_then(|i| self.tokens.get(i)).map(|t| t.kind) != Some(TokenKind::Quote) {
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

            // Parse raw string content as literal text.
            p.parse_raw_string_content(opening_hashes);
        });

        true
    }

    /// Parse the content inside a raw string.
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

            self.bump_raw();
        }
    }

    /// Parse a string or raw string (dispatches to correct method)
    pub(crate) fn parse_any_string(&mut self) -> bool {
        if self.at(TokenKind::Hash) {
            self.parse_raw_string()
        } else if self.at(TokenKind::Quote) {
            self.parse_string()
        } else if self.at(TokenKind::Backtick) {
            self.parse_backtick_string()
        } else {
            false
        }
    }

    /// Parse a backtick-interpolated string literal (BEP-049).
    ///
    /// Lexer emits: Backtick+, (content tokens), Backtick+
    /// Parser counts opening backticks (N), then scans content until it finds
    /// a maximal run of backticks where the run length R ≥ N and the next
    /// token is not a backtick. The trailing N of that run form the close;
    /// the first R - N are content (anchored-close rule, §8 of BEP-049).
    ///
    /// M1: contents are captured verbatim — `${...}` is left as literal tokens
    /// inside the node, to be lifted into `BACKTICK_INTERPOLATION` segments by
    /// M2.
    pub(crate) fn parse_backtick_string(&mut self) -> bool {
        if !self.at(TokenKind::Backtick) {
            return false;
        }

        let opening_ticks = self.count_consecutive_backticks();
        if opening_ticks == 0 {
            return false;
        }

        // BEP-049 §8 case 1: an N-tick opener with N ≥ 2 and no matching
        // close anywhere ahead would silently consume the rest of the file
        // looking for one. The classic offender is `` `` ``, which a user
        // might write meaning "empty string". Detect this at the opener and
        // emit a clean diagnostic instead of running off the end.
        if opening_ticks >= 2 {
            let Some(first_backtick) = self.find_first_backtick_pos() else {
                return false;
            };
            let scan_start = first_backtick + opening_ticks;
            if !self.has_backtick_close_ahead_from(scan_start, opening_ticks) {
                let span = self.tokens[first_backtick].span;
                let ticks = "`".repeat(opening_ticks);
                self.error(
                    format!(
                        "Empty multi-tick backtick string ({ticks}) has no matching {ticks} close. \
                         Use \"\" for empty strings (BEP-049 §8)."
                    ),
                    span,
                );
                // Consume the opener as a degenerate BACKTICK_STRING_LITERAL so
                // the parser advances past it; downstream code keeps parsing
                // instead of being swallowed by an unclosed multi-tick search.
                while self.eat_trivia() {}
                self.with_node(SyntaxKind::BACKTICK_STRING_LITERAL, |p| {
                    for _ in 0..opening_ticks {
                        p.bump();
                    }
                });
                return true;
            }
        }

        // Emit leading trivia outside the node.
        while self.eat_trivia() {}

        self.with_node(SyntaxKind::BACKTICK_STRING_LITERAL, |p| {
            for _ in 0..opening_ticks {
                p.bump(); // opening `
            }

            p.parse_backtick_content(opening_ticks);
        });

        true
    }

    /// Position of the first `Backtick` token at or after `self.current`
    /// (after skipping basic trivia *and* comments). Used by the empty-
    /// multi-tick check; comment-skipping parity with `at(Backtick)` so
    /// backticks preceded by a comment (`/*c*/ <backtick>ok<backtick>`)
    /// don't get rejected.
    fn find_first_backtick_pos(&self) -> Option<usize> {
        let i = self.skip_trivia_and_comments_from(self.current);
        if i < self.tokens.len() && self.tokens[i].kind == TokenKind::Backtick {
            Some(i)
        } else {
            None
        }
    }

    /// Is there a maximal run of ≥ `n` consecutive `Backtick` tokens somewhere
    /// from `start` onward, within the CURRENT top-level item, ignoring
    /// backticks inside `//` and `/* */` comments?
    ///
    /// The scope and comment-skip both matter: without them, an unrelated
    /// backtick literal later in the file (or a backtick run inside a doc
    /// comment showing a markdown code fence) silently satisfies the
    /// empty-multi-tick guard, suppressing the targeted diagnostic and
    /// letting `parse_backtick_content` consume across function boundaries.
    /// Ultrareview bugs 008 and 002.
    ///
    /// Over-approximation: doesn't account for backslash-escape sequences
    /// inside content (e.g. an escaped backtick), so this can return `true`
    /// when the actual close lives behind a backslash. That's fine — false
    /// negatives for the empty-multi-tick check just mean the parser proceeds
    /// normally and either matches a real close or emits the existing
    /// "unclosed backtick string" error.
    fn has_backtick_close_ahead_from(&self, start: usize, n: usize) -> bool {
        let mut i = start;
        while i < self.tokens.len() {
            // Stop at the next top-level item — keywords like `function`,
            // `class`, etc. that always start a fresh item at module scope.
            // A backtick literal lives inside one item; close runs from
            // other items don't count. But this boundary only applies when the
            // keyword *begins a line* (where module items actually start): a
            // keyword sitting mid-line is backtick content (e.g.
            // ````function name````) and must not suppress a valid close run.
            if matches!(
                self.tokens[i].kind,
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
            ) && self.token_starts_line(i)
            {
                return false;
            }

            // Skip over comment patterns (the lexer doesn't emit comment
            // tokens; `//` is two `Slash` tokens, `/* */` is `Slash Star
            // ... Star Slash`). Without skipping, backticks inside a
            // markdown code fence in a doc comment defeat the guard.
            let after_comment = self.skip_comment_at(i);
            if after_comment != i {
                i = after_comment;
                continue;
            }

            if self.tokens[i].kind == TokenKind::Backtick {
                let run_start = i;
                while i < self.tokens.len() && self.tokens[i].kind == TokenKind::Backtick {
                    i += 1;
                }
                if i - run_start >= n {
                    return true;
                }
            } else {
                i += 1;
            }
        }
        false
    }

    /// Whether the token at `idx` is the first non-whitespace token on its line
    /// — preceded only by `Whitespace` back to a `Newline` or the start of
    /// input. Distinguishes a module-item keyword that begins a line from one
    /// appearing mid-line as backtick content.
    fn token_starts_line(&self, idx: usize) -> bool {
        let mut j = idx;
        while j > 0 {
            j -= 1;
            match self.tokens[j].kind {
                TokenKind::Whitespace => continue,
                TokenKind::Newline => return true,
                _ => return false,
            }
        }
        true
    }

    /// Count consecutive `Backtick` tokens at `self.current` (after skipping
    /// basic trivia *and* comments to find the first backtick). The run
    /// itself is required to be uninterrupted — no whitespace or comments
    /// permitted inside the delimiter run.
    ///
    /// Skipping comments mirrors the entry guard `self.at(TokenKind::Backtick)`,
    /// which uses comment-skipping `current()`. Without parity here a literal
    /// like `let s = /*c*/ <backtick>ok<backtick>` would have `at` succeed but
    /// the helper return 0, falsely rejecting the parse.
    fn count_consecutive_backticks(&self) -> usize {
        let mut i = self.skip_trivia_and_comments_from(self.current);
        let mut count = 0;
        while i < self.tokens.len() && self.tokens[i].kind == TokenKind::Backtick {
            count += 1;
            i += 1;
        }
        count
    }

    fn parse_backtick_content(&mut self, opening_ticks: usize) {
        let mut loop_counter = 0;
        loop {
            loop_counter += 1;
            if loop_counter > 1_000_000 {
                self.error_unexpected_token(
                    "Backtick string parsing exceeded iteration limit".to_string(),
                );
                return;
            }

            if self.at_end_raw() {
                self.error_unexpected_token(format!(
                    "Unclosed backtick string (expected {})",
                    "`".repeat(opening_ticks)
                ));
                return;
            }

            // Backslash escape: consume the `\` and the next raw token as
            // content so a `\\\`` does not look like a closing delimiter and
            // `\\${` does not look like an interpolation start.
            //
            // The second consume must take EXACTLY one token — not whatever
            // `bump_raw` happens to land on after skipping trivia. Otherwise
            // `` `\<space>` `` (backslash, space, closing backtick) overshoots:
            // `bump_raw` eats the space as leading trivia and then takes the
            // closing backtick as the escape target, leaving the literal
            // unclosed (ultrareview bug_011).
            if self.at_raw(TokenKind::Backslash) {
                self.bump_raw();
                if self.current < self.tokens.len() {
                    self.bump_one_token_raw();
                }
                continue;
            }

            // BEP-049 §3: `${...}` opens an interpolation. Adjacency required —
            // `$` followed immediately by `{` (no trivia between). A lone `$`
            // or `$ {` is literal content.
            if self.at_raw(TokenKind::Dollar) && self.dollar_immediately_followed_by_lbrace() {
                self.parse_backtick_interpolation();
                continue;
            }

            if self.at_raw(TokenKind::Backtick) {
                let run_len = self.count_consecutive_backticks();
                if run_len >= opening_ticks {
                    // Anchored close: trailing `opening_ticks` ticks of the
                    // run are the close; any leading excess is content.
                    let content_ticks = run_len - opening_ticks;
                    for _ in 0..content_ticks {
                        self.bump_raw();
                    }
                    for _ in 0..opening_ticks {
                        self.bump_raw();
                    }
                    return;
                }
                // run_len < opening_ticks — all of them are content.
                for _ in 0..run_len {
                    self.bump_raw();
                }
                continue;
            }

            // Plain content token (text, whitespace, newline, etc.).
            self.bump_raw();
        }
    }

    /// Inside a backtick string, is the current `$` token immediately
    /// followed by `{` (no trivia between)? Required for `${...}` to start
    /// an interpolation per BEP-049 §3.
    ///
    /// Uses raw token positions (no trivia/comment skipping) for the
    /// adjacency check — `$ {` (space) is *not* an interpolation.
    fn dollar_immediately_followed_by_lbrace(&self) -> bool {
        // Find the raw position of the next non-basic-trivia token (do NOT
        // skip comments — inside a backtick string the `//` pattern is
        // literal text, not a comment).
        let mut i = self.current;
        while i < self.tokens.len() && self.is_basic_trivia(self.tokens[i].kind) {
            i += 1;
        }
        if self.tokens.get(i).map(|t| t.kind) != Some(TokenKind::Dollar) {
            return false;
        }
        self.tokens
            .get(i + 1)
            .is_some_and(|t| t.kind == TokenKind::LBrace)
    }

    /// Parse a `${...}` interpolation inside a backtick string.
    ///
    /// Three forms exist (BEP §4–§5):
    ///   - **Block expression** (M2): `${expr}`, `${ let x = ...; x }`. Body
    ///     is a host block-expression; statements + optional trailing expr.
    ///   - **Block-tag open** (M3): `${for (...)}`, `${if (...)}`. The
    ///     interp closes here; the body comes from subsequent backtick
    ///     content until a matching `${endfor}` / `${endif}`.
    ///   - **Block-tag close / continuation** (M3): `${endfor}`, `${endif}`,
    ///     `${else}`, `${else if (...)}`. Only valid as continuations of an
    ///     open block-tag.
    ///
    /// Dispatch is on the first non-trivia token inside `${...}` (after
    /// `{`). For `if` specifically, the form depends on what follows the
    /// condition — `{` is an if-expression then-block; `}` closes the
    /// interp (block-tag form). `for` is always block-tag (no
    /// for-expression in BAML).
    ///
    /// Pre-condition: `self` is at `$` with `{` adjacent.
    fn parse_backtick_interpolation(&mut self) {
        // Any whitespace/newlines before `$` belong to the surrounding text,
        // not the interpolation. Emit them into the parent BACKTICK_STRING_LITERAL
        // before opening the inner node.
        while self.eat_basic_trivia() {}

        match self.classify_backtick_interp() {
            BacktickInterpForm::For => self.parse_backtick_for_open(),
            BacktickInterpForm::Endfor => {
                self.parse_backtick_simple_tag(SyntaxKind::BACKTICK_ENDFOR, "endfor");
            }
            BacktickInterpForm::IfBlockTag => self.parse_backtick_if_open(),
            BacktickInterpForm::ElseIfBlockTag => self.parse_backtick_else_if(),
            BacktickInterpForm::ElseBlockTag => self.parse_backtick_else(),
            BacktickInterpForm::Endif => {
                self.parse_backtick_simple_tag(SyntaxKind::BACKTICK_ENDIF, "endif");
            }
            BacktickInterpForm::Expression => {
                self.with_node(SyntaxKind::BACKTICK_INTERPOLATION, |p| {
                    p.bump_raw(); // $
                    // `parse_block_expr` consumes its own `{ ... }` and uses
                    // the normal trivia-skipping `at()`/`bump()` — exactly
                    // what we want once we're inside `${`: comments and
                    // whitespace work normally.
                    p.parse_block_expr();
                });
            }
        }
    }

    /// Classify the form of a `${...}` interpolation that starts at the
    /// current `$` token. Cheap pre-parse lookahead — does not consume
    /// any tokens.
    fn classify_backtick_interp(&self) -> BacktickInterpForm {
        // current is at `$`. Peek past `${` to the first interesting token.
        let Some(dollar_idx) = self.current_non_trivia_index() else {
            return BacktickInterpForm::Expression;
        };
        debug_assert_eq!(self.tokens[dollar_idx].kind, TokenKind::Dollar);
        // The `{` is immediately after `$` per the adjacency rule. Skip past it.
        let mut i = dollar_idx + 2;
        // Skip any whitespace/comments inside the interp.
        while i < self.tokens.len()
            && (self.is_basic_trivia(self.tokens[i].kind) || {
                let new_i = self.skip_comment_at(i);
                if new_i != i {
                    i = new_i;
                    true
                } else {
                    false
                }
            })
        {
            if !self.is_basic_trivia(self.tokens[i].kind) {
                continue;
            }
            i += 1;
        }
        if i >= self.tokens.len() {
            return BacktickInterpForm::Expression;
        }
        let first = &self.tokens[i];
        match first.kind {
            TokenKind::For => BacktickInterpForm::For,
            TokenKind::If => {
                if self.classify_backtick_if_is_block_tag(i + 1) {
                    BacktickInterpForm::IfBlockTag
                } else {
                    BacktickInterpForm::Expression
                }
            }
            TokenKind::Else => {
                // `${else}` or `${else if (cond)}`. Look past `else` to
                // disambiguate, skipping comments too (`${else /* c */ if}`) so an
                // interposed comment doesn't misclassify it as a plain `${else}`.
                let j = self.skip_trivia_and_comments_from(i + 1);
                if j < self.tokens.len() && self.tokens[j].kind == TokenKind::If {
                    BacktickInterpForm::ElseIfBlockTag
                } else {
                    BacktickInterpForm::ElseBlockTag
                }
            }
            TokenKind::Word if first.text == "endfor" => BacktickInterpForm::Endfor,
            TokenKind::Word if first.text == "endif" => BacktickInterpForm::Endif,
            _ => BacktickInterpForm::Expression,
        }
    }

    /// Given the index just past an `if` keyword inside a `${if ...}`, decide
    /// whether this is a block-tag form (`}` closes the interp) or an
    /// if-expression form (`{` opens the then-block). Walks tokens with
    /// paren/bracket depth tracking — first `{` at depth 0 means expression,
    /// first `}` at depth 0 means block-tag.
    ///
    /// Limitation: an object-literal in the condition (e.g. `${if Foo { x: 1 }}`)
    /// confuses this exactly like the host `if` parser is confused by the
    /// same shape. Users hitting it should write `${if (Foo { x: 1 })}`.
    fn classify_backtick_if_is_block_tag(&self, start_after_if: usize) -> bool {
        let mut i = start_after_if;
        let mut paren_depth: i32 = 0;
        let mut bracket_depth: i32 = 0;
        while i < self.tokens.len() {
            // Skip comments so a brace inside one (`${if cond /* { */}`) doesn't
            // get mistaken for the then-block opener / interp close.
            let new_i = self.skip_comment_at(i);
            if new_i != i {
                i = new_i;
                continue;
            }
            match self.tokens[i].kind {
                TokenKind::LParen => paren_depth += 1,
                TokenKind::RParen => paren_depth = paren_depth.saturating_sub(1),
                TokenKind::LBracket => bracket_depth += 1,
                TokenKind::RBracket => bracket_depth = bracket_depth.saturating_sub(1),
                TokenKind::LBrace if paren_depth == 0 && bracket_depth == 0 => {
                    return false;
                }
                TokenKind::RBrace if paren_depth == 0 && bracket_depth == 0 => {
                    return true;
                }
                _ => {}
            }
            i += 1;
        }
        false
    }

    /// Parse `${for (let x in xs)}` (block-tag opener). The interp closes
    /// at the `}`; the loop body comes from subsequent backtick content
    /// until a matching `${endfor}`.
    fn parse_backtick_for_open(&mut self) {
        self.with_node(SyntaxKind::BACKTICK_FOR_OPEN, |p| {
            p.bump_raw(); // $
            p.bump_raw(); // {
            p.expect(TokenKind::For);
            // Reuse the host for-header grammar — handles paren / no-paren,
            // iterator / C-style. We only consume the HEADER (no body braces).
            p.parse_for_header_only();
            p.expect(TokenKind::RBrace);
        });
    }

    /// Parse `${if (cond)}` or `${if cond}` (block-tag opener).
    fn parse_backtick_if_open(&mut self) {
        self.with_node(SyntaxKind::BACKTICK_IF_OPEN, |p| {
            p.bump_raw(); // $
            p.bump_raw(); // {
            p.expect(TokenKind::If);
            p.suppress_object_literal_depth += 1;
            p.parse_expr();
            p.suppress_object_literal_depth -= 1;
            p.expect(TokenKind::RBrace);
        });
    }

    /// Parse `${else if (cond)}` (continuation of an open `${if}` block-tag).
    fn parse_backtick_else_if(&mut self) {
        self.with_node(SyntaxKind::BACKTICK_ELSE_IF, |p| {
            p.bump_raw(); // $
            p.bump_raw(); // {
            p.expect(TokenKind::Else);
            p.expect(TokenKind::If);
            p.suppress_object_literal_depth += 1;
            p.parse_expr();
            p.suppress_object_literal_depth -= 1;
            p.expect(TokenKind::RBrace);
        });
    }

    /// Parse `${else}` (continuation of an open `${if}` block-tag).
    fn parse_backtick_else(&mut self) {
        self.with_node(SyntaxKind::BACKTICK_ELSE, |p| {
            p.bump_raw(); // $
            p.bump_raw(); // {
            p.expect(TokenKind::Else);
            p.expect(TokenKind::RBrace);
        });
    }

    /// Parse a `${endfor}` or `${endif}` close tag — a keyword-only interp
    /// whose word is contextual (these aren't host language keywords).
    fn parse_backtick_simple_tag(&mut self, kind: SyntaxKind, expected_word: &str) {
        self.with_node(kind, |p| {
            p.bump_raw(); // $
            p.bump_raw(); // {
            // `endfor` / `endif` aren't lexer keywords — they appear as
            // WORD tokens. Validate the text matches what we expected.
            if p.at(TokenKind::Word) && p.current().map(|t| t.text.as_str()) == Some(expected_word)
            {
                p.bump();
            } else {
                p.error_unexpected_token(format!("'{expected_word}'"));
            }
            p.expect(TokenKind::RBrace);
        });
    }

    /// Parse just the *header* of a for-loop (`for (let x in xs)` minus the
    /// body braces). Used for `${for}` block-tag where the body lives in the
    /// surrounding backtick content rather than inline braces.
    fn parse_for_header_only(&mut self) {
        // Mirrors `parse_for_expr` minus the `expect(For)` and minus the
        // trailing `parse_block_expr`. Supports parenthesized iterator,
        // C-style for, AND non-paren iterator form (`for let x in xs`).
        if self.at(TokenKind::LParen) {
            self.bump(); // (
            // Mirror the host `for` parser: a binding intro is `let` OR the
            // contextual `const`, so `${for (const x in xs)}` parses like host
            // syntax instead of falling into the C-style path and erroring.
            if self.at_binding_intro_stmt() {
                if self.looks_like_for_in_loop() {
                    self.parse_for_in_pattern();
                    self.expect(TokenKind::In);
                    let suppress_object_literal = !self.has_for_header_closing_paren_ahead();
                    if suppress_object_literal {
                        self.suppress_object_literal_depth += 1;
                    }
                    self.parse_expr();
                    if suppress_object_literal {
                        self.suppress_object_literal_depth -= 1;
                    }
                } else {
                    self.parse_let_stmt();
                    if !self.at(TokenKind::Semicolon) && !self.at(TokenKind::RParen) {
                        self.parse_expr();
                    }
                    self.eat(TokenKind::Semicolon);
                    if !self.at(TokenKind::RParen) {
                        self.parse_expr();
                    }
                }
            } else if self.at(TokenKind::Word) || self.at(TokenKind::Semicolon) {
                self.parse_c_style_for_body();
            } else {
                self.error_unexpected_token("'let' or ';'".to_string());
            }
            self.expect(TokenKind::RParen);
        } else {
            // Non-parenthesized iterator form.
            self.parse_for_in_pattern();
            self.expect(TokenKind::In);
            self.suppress_object_literal_depth += 1;
            self.parse_expr();
            self.suppress_object_literal_depth -= 1;
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
                    p.error("attribute is missing a name".to_string(), at_span);
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
        // Function values cannot declare their own generic parameters, so a
        // leading `<...>` on a function type is rejected. Recover by consuming the
        // list and parsing the `(...) -> R` so the rest of the file still parses.
        if self.at(TokenKind::Less) {
            self.error_here("a function type cannot declare generic parameters".to_string());
            self.parse_generic_param_list();
            if self.at(TokenKind::LParen) {
                self.parse_paren_or_function_type(consume_union);
            } else {
                self.error_unexpected_token("function type parameter list".to_string());
            }
            return;
        }

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
                Some(
                    TokenKind::BigintLiteral | TokenKind::IntegerLiteral | TokenKind::FloatLiteral
                )
            )
        {
            let next_kind = self.peek(1).map(|t| t.kind);
            if next_kind == Some(TokenKind::FloatLiteral)
                && let Some(token) = self.peek(1)
            {
                let span = token.span;
                let text = token.text.clone();
                self.error(
                    format!("float literal values are not supported: -{text}"),
                    span,
                );
            }
            self.bump(); // -
            self.bump(); // number
            return;
        }

        // Check for bigint literal types: 42n | 0n | 99999999999999999999n
        if self.at(TokenKind::BigintLiteral) {
            self.bump();
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
                    format!("float literal values are not supported: {}", token.text),
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

            // Consume dot-separated path segments (e.g., baml.http.Request).
            // `spawn`/`await` are reserved keywords but valid as namespace
            // segments after a `.` (e.g. `baml.spawn.SpawnParams` in a type
            // annotation), mirroring `parse_path_or_ident`'s segment set.
            while self.at(TokenKind::Dot) {
                self.bump(); // dot
                if self.at(TokenKind::Word)
                    || self.at(TokenKind::Spawn)
                    || self.at(TokenKind::Await)
                {
                    self.bump(); // next segment
                } else {
                    self.error_unexpected_token("type name segment after '.'".to_string());
                    break;
                }
            }

            // Check for generic arguments: map<K, V>
            if self.at(TokenKind::Less) {
                self.parse_type_args();
            }
        } else if self.at(TokenKind::LParen) {
            // Could be:
            // 1. Parenthesized type: (int | string)
            // 2. Function type: (x: int, y: int) -> bool  OR  (int, int) -> bool
            // 3. Associated type projection: (T as Iterator<T>).Item
            //
            // Projections have their own `as` separator and trailing `.Member`,
            // so parse those before the general paren/function path.
            if self.looks_like_associated_type_projection() {
                self.parse_associated_type_projection();
            } else {
                // We parse the contents as function type parameters (which can be either
                // `name: type` or just `type`), then check for `->` to determine which case.
                self.parse_paren_or_function_type(consume_union);
            }
        } else {
            self.error_unexpected_token("type".to_string());
        }
    }

    fn looks_like_named_type_arg_binding(&self) -> bool {
        let current = self.skip_trivia_and_comments_from(self.current);
        if self
            .tokens
            .get(current)
            .is_none_or(|token| token.kind != TokenKind::Word)
        {
            return false;
        }
        let next = self.skip_trivia_and_comments_from(current + 1);
        self.tokens
            .get(next)
            .is_some_and(|t| t.kind == TokenKind::Equals)
    }

    fn parse_type_arg_or_associated_binding(&mut self) {
        if self.looks_like_named_type_arg_binding() {
            self.with_node(SyntaxKind::ASSOCIATED_TYPE_DECL, |p| {
                // Named associated binding inside a type application:
                // `Iterator<Item = int>`. There is no contextual `type`
                // token here; the name is the associated type being bound.
                p.bump(); // name
                p.expect(TokenKind::Equals);
                p.parse_type();
            });
        } else {
            self.parse_type();
        }
    }

    /// Parse `(Base as Interface).Member` inside the surrounding `TYPE_EXPR`.
    fn parse_associated_type_projection(&mut self) {
        self.expect(TokenKind::LParen);
        self.parse_type();
        if self.at_contextual_kw("as") {
            self.bump_contextual_kw_as("as", SyntaxKind::KW_AS);
        } else {
            self.error_unexpected_token("`as`".to_string());
        }
        self.parse_type();
        self.expect(TokenKind::RParen);
        self.expect(TokenKind::Dot);
        if self.at(TokenKind::Word) {
            self.bump();
        } else {
            self.error_unexpected_token("associated type name".to_string());
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

            // Parse enum variants and attributes. Header comments are ordinary trivia here and
            // are emitted by the next `bump`, just like other line comments.
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
                    // Optional delimiter after a variant. Commas are canonical,
                    // but a semicolon is an unambiguous punctuation slip that
                    // the formatter can repair.
                    if !p.eat(TokenKind::Comma) {
                        p.eat(TokenKind::Semicolon);
                    }
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
                    || p.at(TokenKind::BigintLiteral)
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
                        "enum variants cannot have type annotations".to_string(),
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

            // Parse fields, methods, implements blocks, and attributes. Header comments are
            // ordinary trivia in declaration bodies.
            while !p.at(TokenKind::RBrace) && !p.at_end() {
                if p.consume_function_header_comment_if_allowed() {
                    continue;
                }

                // Error recovery: if we see a top-level keyword (except class-local members),
                // assume we missed a closing brace
                let recover_top_level_item = if p.at(TokenKind::Interface) {
                    p.looks_like_interface_declaration_start()
                } else if p.at(TokenKind::Client) {
                    // `client` is a top-level keyword (`client<llm> Name { … }`),
                    // but also a valid class field name (BEP-049 §10 `ctx.client`
                    // on `Context`). Only treat it as a missing-brace recovery
                    // when it's the `client<…>` declaration form.
                    p.looks_like_client_declaration_start()
                } else {
                    p.at_top_level_keyword()
                };
                if recover_top_level_item
                    && !p.at(TokenKind::Function)
                    && !p.at(TokenKind::Implements)
                    && !p.at(TokenKind::Implement)
                {
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
                } else if p.at(TokenKind::Implements) || p.at(TokenKind::Implement) {
                    // Interface implementation block
                    p.parse_implements_block();
                } else if p.at_member_name() {
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

    // ============ Interface Parsing ============

    /// Parse an interface declaration.
    ///
    /// Syntax:
    /// ```text
    /// interface Name<T> requires I1, I2 {
    ///   field: Type
    ///   function method(p: T) -> R
    ///   function with_default(p: T) -> R { ... }
    /// }
    /// ```
    pub(crate) fn parse_interface(&mut self) {
        self.with_node(SyntaxKind::INTERFACE_DEF, |p| {
            while p.at(TokenKind::AtAt) {
                p.parse_atat_attribute();
            }

            p.expect(TokenKind::Interface);

            if p.at(TokenKind::Word) {
                p.bump(); // name
            } else {
                p.error_unexpected_token("interface name".to_string());
            }

            // Optional generic parameters: <T> or <K, V>
            if p.at(TokenKind::Less) {
                p.parse_generic_param_list();
            }

            // Optional requires clause. Interfaces do not extend each other:
            // `requires` says implementors must also implement the listed interfaces.
            if p.at(TokenKind::Requires) {
                p.parse_requires_clause();
            }

            // Opening brace
            if !p.expect(TokenKind::LBrace) {
                return;
            }

            while !p.at(TokenKind::RBrace) && !p.at_end() {
                if p.consume_function_header_comment_if_allowed() {
                    continue;
                }

                if p.at_top_level_keyword() && !p.at(TokenKind::Function) {
                    break;
                }

                if p.at(TokenKind::AtAt)
                    && p.item_keyword_after_leading_block_attributes() == Some(TokenKind::Function)
                {
                    p.parse_interface_method();
                } else if p.at(TokenKind::AtAt) {
                    p.parse_atat_attribute();
                } else if p.at(TokenKind::Function) {
                    p.parse_interface_method();
                } else if p.at_contextual_kw("type") {
                    p.parse_associated_type_decl(false);
                    if !p.eat(TokenKind::Comma) {
                        p.eat(TokenKind::Semicolon);
                    }
                } else if p.at(TokenKind::Word) {
                    p.parse_field();
                    if !p.eat(TokenKind::Comma) {
                        p.eat(TokenKind::Semicolon);
                    }
                } else {
                    p.error_unexpected_token("Unexpected token in interface body".to_string());
                    p.bump();
                }
            }

            p.expect(TokenKind::RBrace);
        });
    }

    /// Parse a `requires I1, I2, ...` clause inside an interface declaration (BEP-044).
    fn parse_requires_clause(&mut self) {
        self.with_node(SyntaxKind::REQUIRES_CLAUSE, |p| {
            p.expect(TokenKind::Requires);

            if p.is_at_type_start() {
                p.parse_type();
            } else {
                p.error_unexpected_token("interface name".to_string());
            }

            while p.eat(TokenKind::Comma) {
                if p.at(TokenKind::LBrace) {
                    break;
                }
                if p.is_at_type_start() {
                    p.parse_type();
                } else {
                    p.error_unexpected_token("interface name".to_string());
                    break;
                }
            }
        });
    }

    /// Parse a method declaration inside an interface body.
    ///
    /// Two forms:
    /// - Required: `function name(params) -> ReturnType` (no body)
    /// - Default:  `function name(params) -> ReturnType { ... }`
    ///
    /// When there is no body we record a `METHOD_SIG` node so lowering can
    /// distinguish required from default methods syntactically.
    fn parse_interface_method(&mut self) {
        // Speculative scan: does this method have a body?
        let has_body = self.interface_method_has_body();

        if has_body {
            // Full FUNCTION_DEF — reuse existing parser so default methods are
            // structurally identical to regular methods downstream.
            self.parse_function();
            return;
        }

        self.with_node(SyntaxKind::METHOD_SIG, |p| {
            while p.at(TokenKind::AtAt) {
                p.parse_atat_attribute();
            }

            p.expect(TokenKind::Function);

            // Accept BEP-044 keyword tokens as method names — see
            // the matching block in `parse_function`.
            if p.at(TokenKind::Word)
                || p.at(TokenKind::Implements)
                || p.at(TokenKind::Implement)
                || p.at(TokenKind::Extends)
                || p.at(TokenKind::Requires)
                || p.at(TokenKind::Interface)
            {
                p.bump();
            } else {
                p.error_unexpected_token("method name".to_string());
            }

            // Optional generic parameters
            if p.at(TokenKind::Less) {
                p.parse_generic_param_list();
            }

            p.parse_parameter_list();

            if p.eat(TokenKind::Arrow) {
                p.parse_type();
            } else {
                p.error_unexpected_token("return type (->)".to_string());
            }

            if p.at(TokenKind::Throws) {
                p.with_node(SyntaxKind::THROWS_CLAUSE, |p| {
                    p.bump();
                    p.parse_type();
                });
            }
        });
    }

    /// Look ahead from the current `function` token to see whether the
    /// declaration ends in `{ ... }` (default impl) or just at the next
    /// declaration boundary (required signature only).
    ///
    /// `self.current` may point at trivia before the `function` keyword
    /// (because `at()` skips trivia). We therefore find the first non-trivia
    /// token (expected to be `function`), then scan from the *next* token
    /// looking for an `LBrace` at brace/paren/bracket/angle depth 0.
    fn interface_method_has_body(&self) -> bool {
        self.function_body_start().is_some()
    }

    /// Locate the opening brace of the function declaration at the current position.
    ///
    /// This accepts leading comment trivia and block attributes, and stops at the next declaration
    /// boundary so a required interface signature cannot borrow a later function's body.
    fn function_body_start(&self) -> Option<usize> {
        // Locate the `function` token we're about to consume.
        let mut i = self.current;
        loop {
            let next = self.skip_trivia_and_comments_from(i);
            if next == i {
                break;
            }
            i = next;
        }

        while self.tokens.get(i).map(|t| t.kind) == Some(TokenKind::AtAt) {
            let Some(next) = self.skip_block_attribute_from(i) else {
                break;
            };
            i = next;
        }

        i = self.skip_trivia_and_comments_from(i);
        if self.tokens.get(i).map(|token| token.kind) != Some(TokenKind::Function) {
            return None;
        }
        i += 1;

        let mut paren_depth: i32 = 0;
        let mut bracket_depth: i32 = 0;
        let mut angle_depth: i32 = 0;

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
                TokenKind::LParen => paren_depth += 1,
                TokenKind::RParen => paren_depth -= 1,
                TokenKind::LBracket => bracket_depth += 1,
                TokenKind::RBracket => bracket_depth -= 1,
                // Generic delimiters matter in declaration-level generic parameters and return
                // types. Inside parameter parentheses these tokens may instead be comparison or
                // shift operators in default expressions; paren depth already prevents an inner
                // brace from being mistaken for the function body.
                TokenKind::Less if paren_depth == 0 => angle_depth += 1,
                TokenKind::Greater if paren_depth == 0 => {
                    angle_depth = angle_depth.saturating_sub(1);
                }
                TokenKind::GreaterGreater if paren_depth == 0 => {
                    angle_depth = angle_depth.saturating_sub(2);
                }
                TokenKind::LBrace if paren_depth == 0 && bracket_depth == 0 && angle_depth == 0 => {
                    return Some(i);
                }
                TokenKind::RBrace if paren_depth == 0 && bracket_depth == 0 && angle_depth == 0 => {
                    // End of the interface body without finding a body for
                    // this method — it's a required signature.
                    return None;
                }
                // Encountering the start of another interface member at the
                // outer level means we're done with this signature.
                TokenKind::Function | TokenKind::AtAt
                    if paren_depth == 0 && bracket_depth == 0 && angle_depth == 0 =>
                {
                    return None;
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    /// Whether the raw header at the current position immediately precedes an expression function.
    fn header_precedes_expression_function(&self) -> bool {
        self.at_header_comment_start()
            && self
                .function_body_start()
                .is_some_and(|body_start| !self.looks_like_llm_function_body_from(body_start))
    }

    fn consume_function_header_comment_if_allowed(&mut self) -> bool {
        if !self.header_precedes_expression_function() {
            return false;
        }
        self.consume_function_header_comment();
        true
    }

    /// Parse an `implements I { ... }` (or `implement I { ... }`) block inside a class body.
    fn parse_implements_block(&mut self) {
        self.with_node(SyntaxKind::IMPLEMENTS_BLOCK, |p| {
            if p.at(TokenKind::Implement) {
                p.bump();
            } else {
                p.expect(TokenKind::Implements);
            }

            // Target interface — capture as IMPLEMENTS_TARGET so lowering
            // can address it directly even when the type is generic.
            p.with_node(SyntaxKind::IMPLEMENTS_TARGET, |p| {
                if p.is_at_type_start() {
                    p.parse_type();
                } else {
                    p.error_unexpected_token("interface name".to_string());
                }
            });

            if !p.expect(TokenKind::LBrace) {
                return;
            }

            while !p.at(TokenKind::RBrace) && !p.at_end() {
                if p.consume_function_header_comment_if_allowed() {
                    continue;
                }

                if p.at_top_level_keyword() && !p.at(TokenKind::Function) {
                    break;
                }
                if p.at(TokenKind::Function)
                    || (p.at(TokenKind::AtAt)
                        && p.item_keyword_after_leading_block_attributes()
                            == Some(TokenKind::Function))
                {
                    p.parse_function();
                } else if p.at_contextual_kw("type") {
                    p.parse_associated_type_decl(true);
                    if !p.eat(TokenKind::Comma) {
                        p.eat(TokenKind::Semicolon);
                    }
                } else if p.looks_like_interface_field_link() {
                    p.parse_interface_field_link();
                    if !p.eat(TokenKind::Comma) {
                        p.eat(TokenKind::Semicolon);
                    }
                } else if p.at_member_name() {
                    p.parse_field();
                    if !p.eat(TokenKind::Comma) {
                        p.eat(TokenKind::Semicolon);
                    }
                } else {
                    p.error_unexpected_token(
                        "field or method definition expected in `implements` block".to_string(),
                    );
                    p.bump();
                }
            }

            p.expect(TokenKind::RBrace);
        });
    }

    /// Parse an explicit interface-field link inside an `implements` block:
    /// `interface_field as class_field`.
    fn parse_interface_field_link(&mut self) {
        self.with_node(SyntaxKind::INTERFACE_FIELD_LINK, |p| {
            if p.at_member_name() {
                p.bump();
            } else {
                p.error_unexpected_token("interface field name".to_string());
            }

            if p.at_contextual_kw("as") {
                p.bump_contextual_kw_as("as", SyntaxKind::KW_AS);
            } else {
                p.error_unexpected_token("`as`".to_string());
            }

            if p.at_member_name() {
                p.bump();
            } else {
                p.error_unexpected_token("class field name".to_string());
            }
        });
    }

    /// Parse a top-level `implements I for T { ... }` block.
    fn parse_implements_for(&mut self) {
        self.with_node(SyntaxKind::IMPLEMENTS_FOR, |p| {
            if p.at(TokenKind::Implement) {
                p.bump();
            } else {
                p.expect(TokenKind::Implements);
            }

            if p.at(TokenKind::Less) {
                p.parse_generic_param_list();
            }

            // Interface target (reuse IMPLEMENTS_TARGET).
            p.with_node(SyntaxKind::IMPLEMENTS_TARGET, |p| {
                if p.is_at_type_start() {
                    p.parse_type();
                } else {
                    p.error_unexpected_token("interface name".to_string());
                }
            });

            p.expect(TokenKind::For);

            // Target type.
            p.with_node(SyntaxKind::IMPLEMENTS_FOR_TARGET, |p| {
                if p.is_at_type_start() {
                    p.parse_type();
                } else {
                    p.error_unexpected_token("target type".to_string());
                }
            });

            if !p.expect(TokenKind::LBrace) {
                return;
            }

            while !p.at(TokenKind::RBrace) && !p.at_end() {
                if p.consume_function_header_comment_if_allowed() {
                    continue;
                }

                if p.at_top_level_keyword() && !p.at(TokenKind::Function) {
                    break;
                }
                if p.at(TokenKind::Function)
                    || (p.at(TokenKind::AtAt)
                        && p.item_keyword_after_leading_block_attributes()
                            == Some(TokenKind::Function))
                {
                    p.parse_function();
                } else if p.at_contextual_kw("type") {
                    p.parse_associated_type_decl(true);
                    if !p.eat(TokenKind::Comma) {
                        p.eat(TokenKind::Semicolon);
                    }
                } else if p.looks_like_interface_field_link() {
                    p.parse_interface_field_link();
                    if !p.eat(TokenKind::Comma) {
                        p.eat(TokenKind::Semicolon);
                    }
                } else if p.at_member_name() {
                    p.parse_field();
                    if !p.eat(TokenKind::Comma) {
                        p.eat(TokenKind::Semicolon);
                    }
                } else {
                    p.error_unexpected_token(
                        "field or method definition expected in `implements` block".to_string(),
                    );
                    p.bump();
                }
            }

            p.expect(TokenKind::RBrace);
        });
    }

    /// Parse BEP-057 associated type declarations and bindings:
    /// - interface body: `type Item`, `type Item extends Bound`, `type Item = Default`
    /// - implements body: `type Item = Concrete`
    fn parse_associated_type_decl(&mut self, require_binding: bool) {
        self.with_node(SyntaxKind::ASSOCIATED_TYPE_DECL, |p| {
            if p.at_contextual_kw("type") {
                p.bump_contextual_kw_as("type", SyntaxKind::KW_TYPE);
            } else {
                p.error_unexpected_token("`type`".to_string());
            }

            if p.at(TokenKind::Word) {
                p.bump();
            } else {
                p.error_unexpected_token("associated type name".to_string());
            }

            if p.at(TokenKind::Extends) {
                if require_binding && let Some(span) = p.current().map(|t| t.span) {
                    p.error(
                        "associated type bounds are only allowed on interface declarations"
                            .to_string(),
                        span,
                    );
                }
                p.bump();
                if p.is_at_type_start() {
                    p.parse_type();
                } else {
                    p.error_unexpected_token("associated type bound".to_string());
                }
            }

            if p.eat(TokenKind::Equals) {
                if p.is_at_type_start() {
                    p.parse_type();
                } else {
                    p.error_unexpected_token("associated type binding".to_string());
                }
            } else if require_binding {
                p.error_unexpected_token("associated type binding (`= Type`)".to_string());
            }
        });
    }

    /// Parse declaration-site generic parameter list: `<T>` or `<K, V>`.
    ///
    /// This is different from `GENERIC_ARGS` (call-site: `fetch<Response>(url)`).
    /// This produces `GENERIC_PARAM_LIST` containing `GENERIC_PARAM` children.
    fn parse_generic_param_list(&mut self) {
        self.type_args_depth += 1;
        self.with_node(SyntaxKind::GENERIC_PARAM_LIST, |p| {
            p.expect(TokenKind::Less); // <

            // Parse comma-separated type parameter names
            loop {
                if p.at(TokenKind::Greater) || p.at(TokenKind::GreaterGreater) || p.at_end() {
                    break;
                }
                p.with_node(SyntaxKind::GENERIC_PARAM, |p| {
                    if p.at(TokenKind::Word) {
                        p.bump(); // type parameter name
                    } else {
                        p.error_unexpected_token("type parameter name".to_string());
                    }

                    // BEP-044 generic bounds: `<T extends Iface>` or
                    // intersection `<T extends A & B>`. The bounds are
                    // captured as a `GENERIC_PARAM_BOUNDS` wrapper holding
                    // one or more `TYPE_EXPR` children, separated by `&`.
                    if p.at(TokenKind::Extends) {
                        p.with_node(SyntaxKind::GENERIC_PARAM_BOUNDS, |p| {
                            p.expect(TokenKind::Extends);
                            // First bound type.
                            p.parse_type();
                            // Optional `& Other & ...` intersection.
                            while p.eat(TokenKind::And) {
                                p.parse_type();
                            }
                            if p.at_contextual_kw("as") {
                                if let Some(token) = p.current() {
                                    p.error(
                                        "generic bound aliases are not supported; use `.as<Interface<...>>()` at call sites".to_string(),
                                        token.span,
                                    );
                                }
                                p.bump();
                                if p.at(TokenKind::Word) {
                                    p.bump();
                                } else {
                                    p.error_unexpected_token("alias name".to_string());
                                }
                            }
                        });
                    }
                });
                if !p.eat(TokenKind::Comma) {
                    break;
                }
            }

            p.expect_greater(); // >
        });
        self.type_args_depth -= 1;

        // If we just exited the outermost generic and have pending '>', report error.
        if self.type_args_depth == 0 && self.pending_greaters > 0 {
            if let Some(span) = self.pending_greater_span {
                self.error(
                    format!(
                        "unmatched `>` in generic parameter list (found {} extra)",
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

            // Function name. The lexer produces dedicated keyword tokens for
            // BEP-044 syntax, but those words remain valid as method names —
            // notably on the `TypeValue` reflection class. Accept them here.
            if p.at(TokenKind::Word)
                || p.at(TokenKind::Implements)
                || p.at(TokenKind::Implement)
                || p.at(TokenKind::Extends)
                || p.at(TokenKind::Requires)
                || p.at(TokenKind::Interface)
            {
                p.bump();
            } else {
                p.error_unexpected_token("function name".to_string());
                // Recovery: skip until we see '(', '{', or '->'
                while !p.at(TokenKind::LParen)
                    && !p.at(TokenKind::Less)
                    && !p.at(TokenKind::LBrace)
                    && !p.at(TokenKind::Arrow)
                    && !p.at(TokenKind::FatArrow)
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
                    "old-style function syntax; use `function Name(params...) -> ReturnType { ... }`"
                        .to_string(),
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
            // Accept the common JS/TS `=>` slip as well as canonical `->`.
            // The formatter owns canonicalization and always emits `->`, just
            // as it already does for lambda expressions.
            if p.eat(TokenKind::Arrow) || p.eat(TokenKind::FatArrow) {
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
        self.looks_like_llm_function_body_from(self.current)
    }

    fn looks_like_llm_function_body_from(&self, mut i: usize) -> bool {
        let mut brace_depth = 0;

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
                TokenKind::LBrace => brace_depth += 1,
                TokenKind::RBrace if brace_depth == 1 => break,
                TokenKind::RBrace => brace_depth -= 1,
                TokenKind::Word if brace_depth == 1 => {
                    let text = &token.text;
                    if text == "const" {
                        return false;
                    }
                    // `tools` is deliberately NOT a trigger: every LLM function
                    // must also declare `client` and `prompt`, and raw string
                    // contents lex as ordinary tokens in this scan, so a body
                    // containing e.g. "tools/list" would misclassify.
                    if text == "client" || text == "prompt" {
                        // An LLM field is `client <value>` / `prompt <template>`.
                        // A following `=`, `,`, `)`, or `.` means a named call
                        // arg, plain identifier use, or a member access, and a
                        // following `(` is a call (e.g. `spec.prompt()`), so
                        // those are expression bodies — a field value never
                        // starts with any of them.
                        let j = self.skip_trivia_and_comments_from(i + 1);
                        let next = self.tokens.get(j).map(|t| t.kind);
                        if !matches!(
                            next,
                            Some(
                                TokenKind::Equals
                                    | TokenKind::Comma
                                    | TokenKind::RParen
                                    | TokenKind::Dot
                                    | TokenKind::LParen
                            )
                        ) {
                            return true;
                        }
                    }
                }
                // `client` as KW_CLIENT: the LLM directive is `client Model`,
                // not `client.method(...)`, the named call arg `client = ...`,
                // or a call THROUGH a parameter named `client` — `client(...)`.
                TokenKind::Client if brace_depth == 1 => {
                    let j = self.skip_trivia_and_comments_from(i + 1);
                    let next = self.tokens.get(j).map(|t| t.kind);
                    if !matches!(
                        next,
                        Some(
                            TokenKind::Dot
                                | TokenKind::Equals
                                | TokenKind::Comma
                                | TokenKind::RParen
                                | TokenKind::LParen
                        )
                    ) {
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
            let mut has_tools = false;
            let mut has_type_builder = false;

            while !p.at(TokenKind::RBrace) && !p.at_end() {
                // Error recovery: if we see a top-level keyword (except Client and TypeBuilder)
                // assume we missed a closing brace
                if p.at_top_level_keyword()
                    && !p.at(TokenKind::Client)
                    && !p.at(TokenKind::TypeBuilder)
                {
                    break;
                }

                if p.at(TokenKind::Client) {
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
                } else if p.at(TokenKind::Word)
                    && p.current().map(|t| t.text == "tools").unwrap_or(false)
                {
                    if has_tools {
                        p.error_unexpected_token("Duplicate 'tools' field".to_string());
                    }
                    has_tools = true;
                    p.parse_tools_field();
                } else if p.at(TokenKind::TypeBuilder) {
                    if has_type_builder {
                        p.error_unexpected_token("Duplicate 'type_builder' block".to_string());
                    }
                    has_type_builder = true;
                    // Parse type_builder block - HIR will emit proper error for non-test context
                    p.parse_type_builder_block();
                } else if p.at(TokenKind::Comma) || p.at(TokenKind::Semicolon) {
                    // LLM-block fields are separated by a newline, not by `,`/`;`.
                    // Name the real requirement here instead of letting the value
                    // scanner swallow the separator and misreport `prompt` as
                    // missing (B-621).
                    let sep = p.current().map(|t| t.text.clone()).unwrap_or_default();
                    p.error_here(format!(
                        "unexpected `{sep}` between LLM function fields; separate `client` and `prompt` with a newline"
                    ));
                    p.bump();
                } else {
                    // Unexpected token in LLM function
                    p.error_unexpected_token(format!(
                        "Only 'client', 'tools' and 'prompt' allowed in LLM function, found '{}'",
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

            // The client value is either:
            // - A quoted "provider/model" string: client "openai/gpt-4o"
            // - An expression evaluating to an ai.Client: a declared client
            //   name (`client Fast`), a constructor call
            //   (`client openai.OpenAiClient.new(...)`), a wrapper
            //   (`client ai.Retry.new(Fast)`), etc.
            //
            // The unquoted `client openai/gpt-4o` shorthand of the legacy
            // world is no longer special-cased: as an expression it would be
            // a division of two unresolved names, so lowering rejects a
            // division-shaped client value with a "quote the model string"
            // migration error rather than letting E0003 cascade.
            if p.at(TokenKind::Quote) {
                p.parse_string();
            } else if p.at(TokenKind::RBrace) || p.has_newline_ahead() || p.at_end() {
                p.error_unexpected_token("client value".to_string());
            } else {
                p.parse_expr();
            }
        });
    }

    /// Parse the `tools` field of an LLM function body: `tools [a, b]` or
    /// `tools: [a, b]`. The value is an arbitrary expression producing the
    /// tool list (usually an array literal of function references).
    fn parse_tools_field(&mut self) {
        self.with_node(SyntaxKind::TOOLS_FIELD, |p| {
            // 'tools' keyword (as Word token)
            if p.at(TokenKind::Word) && p.current().map(|t| t.text == "tools").unwrap_or(false) {
                p.bump();
            } else {
                p.error_unexpected_token("'tools' keyword".to_string());
            }

            // Optional colon
            p.eat(TokenKind::Colon);

            p.parse_expr();
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
            // Lambdas are function *values* and cannot declare generic parameters;
            // a leading `<...>` is rejected. Recover by consuming the list.
            if p.at(TokenKind::Less) {
                p.error_here("a lambda cannot declare generic parameters".to_string());
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

            // Parse statements until closing brace. Inspect raw headers before `at(RBrace)`,
            // because ordinary lookahead treats headers as invisible line-comment trivia.
            loop {
                // Handle MDX-style header comments (//#...) as structural statements only in
                // executable block expressions.
                if p.at_header_comment_start() {
                    p.consume_header_comment();
                    continue;
                }
                if p.at_end() || p.at(TokenKind::RBrace) {
                    break;
                }

                // Error recovery: if we see a top-level keyword, assume we missed a closing brace
                if p.at_top_level_keyword_except_client() {
                    break;
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

        if self.at_binding_intro_stmt() {
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
        } else if self.at(TokenKind::Defer) {
            self.parse_defer_stmt();
        } else if self.at(TokenKind::Test) && self.looks_like_test_expr_body() {
            if self.testset_body_depth > 0 {
                self.parse_test_expr();
            } else {
                self.error_here(
                    "test blocks are only allowed at the top level or inside a testset".to_string(),
                );
                self.parse_test_expr(); // still parse to recover
            }
        } else if self.at(TokenKind::TestSet) {
            if self.testset_body_depth > 0 {
                self.parse_testset();
            } else {
                self.error_here(
                    "testset blocks are only allowed at the top level or inside a testset"
                        .to_string(),
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
            if !p.at_binding_intro_stmt() {
                p.error_unexpected_token("'let'".to_string());
            }
            if p.binding_intro_is_followed_by_array_pattern() {
                p.bump_binding_intro(); // statement-level binding introducer
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

            // Optional `let … else { … }` — refutable binding whose else
            // branch must diverge. Allowed in every position `parse_let_stmt`
            // is called from, including C-style for-init (a diverging init
            // just makes the loop unreachable — same kind of dead code we
            // already accept elsewhere, not a parse-time concern).
            if p.at(TokenKind::Else) {
                p.bump(); // else
                if p.at(TokenKind::If) {
                    // `else if` after `let … else` has no value to chain
                    // through (unlike `if let`, which produces a value), so
                    // reject. The `if` token is left in place so recovery can
                    // parse it as the next statement.
                    p.error_unexpected_token(
                        "block after 'else' (`else if` is not valid in let-else)".to_string(),
                    );
                } else if p.at(TokenKind::LBrace) {
                    p.parse_block_expr();
                } else {
                    p.error_unexpected_token("block after 'else'".to_string());
                }
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

    /// Parse `defer { body }` (BEP-042). The body is always a brace-delimited
    /// block; it runs on every exit of the enclosing block (normal completion,
    /// `return`, `break`/`continue`, and error unwinding) in LIFO order. The
    /// CST shape is `DEFER_STMT [ KW_DEFER BLOCK_EXPR ]`.
    fn parse_defer_stmt(&mut self) {
        self.with_node(SyntaxKind::DEFER_STMT, |p| {
            p.expect(TokenKind::Defer);
            if p.at(TokenKind::LBrace) {
                p.parse_block_expr();
            } else {
                p.error_unexpected_token("'{' after defer".to_string());
            }
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

    /// Parse `return expr?` in expression position as a `RETURN_EXPR` — a
    /// diverging expression of type `never`. This is what lets a braceless
    /// `return` be a `catch`/`match` arm value (`_ => return 0`). Statement
    /// position is still handled by `parse_return_stmt` (see `parse_stmt`),
    /// so this only fires when `return` is reached through expression parsing.
    ///
    /// Like `parse_return_stmt`, the value is optional: a bare `return` (e.g.
    /// in a void function) is valid, so we stop before a token that cannot
    /// begin an expression. We don't eat a trailing `;` here — that belongs to
    /// the enclosing statement, not the expression.
    fn parse_return_expr(&mut self) {
        self.with_node(SyntaxKind::RETURN_EXPR, |p| {
            p.expect(TokenKind::Return);

            // Optional return value — bare `return` yields unit before diverging.
            if !p.at(TokenKind::Semicolon)
                && !p.at(TokenKind::Comma)
                && !p.at(TokenKind::RBrace)
                && !p.at_end()
            {
                p.parse_expr_bp_no_catch(0);
            }
        });
    }

    /// Parse `break` in expression position as a `BREAK_EXPR` — a diverging
    /// expression of type `never`, mirroring `parse_return_expr`. This is what
    /// lets a braceless `break` be a `catch`/`match` arm value (`0 => break`).
    /// Statement position is still handled by `parse_break_stmt` (see
    /// `parse_stmt`), so this only fires when `break` is reached through
    /// expression parsing. Unlike the statement form, we don't eat a trailing
    /// `;` here — that belongs to the enclosing statement, not the expression.
    fn parse_break_expr(&mut self) {
        self.with_node(SyntaxKind::BREAK_EXPR, |p| {
            p.expect(TokenKind::Break);
        });
    }

    /// Parse `continue` in expression position as a `CONTINUE_EXPR` — the
    /// `continue` counterpart of `parse_break_expr`. Statement-position
    /// `continue` is still handled by `parse_continue_stmt`.
    fn parse_continue_expr(&mut self) {
        self.with_node(SyntaxKind::CONTINUE_EXPR, |p| {
            p.expect(TokenKind::Continue);
        });
    }

    fn parse_if_expr(&mut self) {
        // `if let PATTERN = SCRUTINEE { ... }` is a distinct refutable form.
        // Decided here by peeking past `if` for a `let` token — patterns
        // always start with `let` (BAML's binding marker), and no normal
        // expression starts with `let`, so this is unambiguous.
        if self.peek_is_binding_intro_stmt(1) {
            self.parse_if_let_expr();
            return;
        }

        self.with_node(SyntaxKind::IF_EXPR, |p| {
            p.expect(TokenKind::If);

            // Condition: suppress destructure patterns (`Class { … }`) so
            // they don't eat the then-block. Mirrors Rust's
            // `NO_STRUCT_LITERAL` restriction. Users who want a
            // destructure here must wrap it in parens — parens reset the
            // suppression in `parse_pattern_atom`.
            p.suppress_destructure_pattern_depth += 1;
            p.suppress_object_literal_depth += 1;
            p.parse_expr();
            p.suppress_object_literal_depth -= 1;
            p.suppress_destructure_pattern_depth -= 1;

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

    /// Parse `if let PATTERN = SCRUTINEE { ... } else { ... }`.
    ///
    /// Caller has already verified the token sequence starts with `if let`.
    /// The `let` itself is consumed by `parse_pattern` (BAML patterns carry
    /// their own leading `let` binding marker), matching how `let` statements
    /// parse.
    fn parse_if_let_expr(&mut self) {
        self.with_node(SyntaxKind::IF_LET_EXPR, |p| {
            p.expect(TokenKind::If);
            // Pattern. For top-level array patterns (`let [a, b]`), the
            // `let` keyword has to be consumed at the statement level
            // because `parse_let_pattern` only handles binding /
            // destructure shapes after `let`. Mirrors `parse_let_stmt`.
            if p.binding_intro_is_followed_by_array_pattern() {
                p.bump_binding_intro(); // statement-level binding introducer
                p.parse_pattern();
            } else {
                // The introducer is consumed inside parse_pattern → parse_let_pattern.
                p.parse_pattern();
            }

            if !p.eat(TokenKind::Equals) {
                p.error_unexpected_token("'=' after if-let pattern".to_string());
            }

            // Scrutinee — exclude assignment operators (same as let-stmt),
            // and apply condition-position destructure suppression so a
            // trailing `is Class { ... }` doesn't eat the then-block.
            p.suppress_destructure_pattern_depth += 1;
            p.suppress_object_literal_depth += 1;
            p.parse_expr_bp(3);
            p.suppress_object_literal_depth -= 1;
            p.suppress_destructure_pattern_depth -= 1;

            // Then block
            if p.at(TokenKind::LBrace) {
                p.parse_block_expr();
            } else {
                p.error_unexpected_token("block after if-let scrutinee".to_string());
            }

            // Optional else, with `else if` / `else if let` chaining.
            if p.at(TokenKind::Else) {
                p.bump(); // else

                if p.at(TokenKind::If) {
                    p.parse_if_expr();
                } else if p.at(TokenKind::LBrace) {
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

            // Scrutinee expression — parens are optional, mirroring `if`
            // and `while`. The `: Type` annotation is only accepted in the
            // parenthesized form (the non-paren form ends the scrutinee at
            // the `{` of the match body, so `match x: Type { ... }` would
            // be ambiguous with a type-ascribed binding).
            if p.at(TokenKind::LParen) {
                p.bump(); // (
                p.parse_expr();
                if p.eat(TokenKind::Colon) {
                    p.parse_type();
                }
                p.expect(TokenKind::RParen);
            } else {
                // No-paren form: scrutinee runs until the `{` of the match
                // body. Suppress the object-literal postfix so a scrutinee
                // like `match Foo { ... }` (where `Foo` happens to look
                // like a constructor) doesn't gobble the match body's
                // brace. Mirrors `spawn`'s approach.
                p.suppress_object_literal_depth += 1;
                p.parse_expr();
                p.suppress_object_literal_depth -= 1;
            }

            // Match body with arms
            if p.at(TokenKind::LBrace) {
                p.bump(); // {

                let mut parsed_any_arm = false;
                loop {
                    // Inspect raw headers before `at(RBrace)`, whose normal lookahead skips them.
                    if p.at_header_comment_start() {
                        p.consume_header_comment();
                        continue;
                    }
                    if p.at_end() || p.at(TokenKind::RBrace) {
                        break;
                    }

                    // Error recovery: if we see a top-level keyword, assume we missed a closing brace
                    if p.at_top_level_keyword_except_client() {
                        break;
                    }
                    let before = p.current;
                    p.parse_match_arm();
                    parsed_any_arm = true;
                    // An arm that consumed nothing already emitted an error;
                    // force progress so the loop cannot spin forever.
                    if p.current == before {
                        p.bump();
                    }
                }
                if !parsed_any_arm {
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
            if self.looks_like_associated_type_projection() {
                self.with_node(SyntaxKind::TYPE_PATTERN, |p| {
                    p.parse_type_with(/* consume_union = */ false);
                });
            } else if self.looks_like_function_type_paren() {
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
                    // Parens reset destructure suppression: `if (x is Foo
                    // { f })` should still allow the destructure.
                    let saved = std::mem::take(&mut p.suppress_destructure_pattern_depth);
                    p.parse_pattern();
                    p.suppress_destructure_pattern_depth = saved;
                    p.expect(TokenKind::RParen);
                });
            }
            return;
        }

        if self.at_binding_intro_pattern() {
            self.parse_let_pattern();
            return;
        }

        if self.at(TokenKind::LBracket) {
            // Arrays use `[` / `]` so the closing bracket terminates the
            // atom cleanly — sub-patterns inside should regain normal
            // destructure parsing.
            let saved = std::mem::take(&mut self.suppress_destructure_pattern_depth);
            self.parse_array_pattern();
            self.suppress_destructure_pattern_depth = saved;
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
        // In condition position (`if x is Class { … }`) we suppress this
        // so the `{` stays available for the outer block; users can still
        // write destructure via `(x is Class { f })`.
        if self.at(TokenKind::Word)
            && self.suppress_destructure_pattern_depth == 0
            && self.looks_like_destructure_pattern()
        {
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
                    | TokenKind::BigintLiteral
                    | TokenKind::IntegerLiteral
                    | TokenKind::FloatLiteral
                    | TokenKind::Quote
                    | TokenKind::Hash
                    | TokenKind::Minus
                    | TokenKind::LParen
                    | TokenKind::LBracket
                    | TokenKind::Less
                    | TokenKind::Let
            )
        ) || self.at_contextual_kw("const")
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
        let preceded_by_newline = self.newline_before_next_non_trivia(close + 1);
        Self::is_generic_args_follow(self.tokens.get(follow).map(|t| t.kind), preceded_by_newline)
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
                | TokenKind::Equals
                | TokenKind::LBracket
                | TokenKind::RBracket
                | TokenKind::Question
                | TokenKind::Pipe
                | TokenKind::BigintLiteral
                | TokenKind::IntegerLiteral
                | TokenKind::FloatLiteral
                | TokenKind::Minus
                | TokenKind::Quote
                | TokenKind::Hash
                | TokenKind::LParen
                | TokenKind::RParen
                // `spawn`/`await` are valid namespace segments inside type
                // args (`foo<baml.spawn.SpawnParams<T, E>>(x)`), mirroring
                // the type-path parser's segment set.
                | TokenKind::Spawn
                | TokenKind::Await => {}
                _ => return None,
            }
            i = self.skip_trivia_and_comments_from(i + 1);
        }
        None
    }

    /// Parse a binding-introducer-prefixed pattern. Either:
    /// - `let _` / `const _` — `WILDCARD_PATTERN`
    /// - `let WORD` / `const WORD` — simple `BINDING_PATTERN`
    /// - `let PATH { fields }` / `const PATH { fields }` — `DESTRUCTURE_PATTERN`
    fn parse_let_pattern(&mut self) {
        debug_assert!(self.at_binding_intro());
        let start = self.events.len();
        self.bump_binding_intro();

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
        // Same condition-position suppression as `parse_pattern_atom` —
        // `if x is let Class { f }` would otherwise eat the then-block.
        if self.suppress_destructure_pattern_depth == 0 && self.looks_like_destructure_pattern() {
            self.parse_path();
            if self.at(TokenKind::Less) && self.looks_like_generic_args() {
                self.parse_type_args();
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
                p.parse_type_args();
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
        self.at(TokenKind::Catch)
            || self.at(TokenKind::CatchAll)
            // `catch_all_panics` is a contextual keyword: the lexer treats it as
            // a plain identifier so it stays usable as one elsewhere, but in
            // catch-clause position it introduces a clause like `catch_all`.
            || self.at_contextual_kw("catch_all_panics")
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
            if p.at(TokenKind::Catch) || p.at(TokenKind::CatchAll) {
                p.bump();
            } else if p.at_contextual_kw("catch_all_panics") {
                // Re-label the contextual `Word` as a dedicated keyword so the
                // AST lowering and tooling can recognize the clause kind.
                p.bump_contextual_kw_as("catch_all_panics", SyntaxKind::KW_CATCH_ALL_PANICS);
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

            let mut parsed_any_arm = false;
            loop {
                // Inspect raw headers before `at(RBrace)`, whose normal lookahead skips them.
                if p.at_header_comment_start() {
                    p.consume_header_comment();
                    continue;
                }
                if p.at_end() || p.at(TokenKind::RBrace) {
                    break;
                }

                if p.at_top_level_keyword_except_client() {
                    break;
                }
                let before = p.current;
                p.parse_catch_arm();
                parsed_any_arm = true;
                // An arm that consumed nothing already emitted an error (e.g.
                // recovery bailed before advancing); force progress so the
                // loop cannot spin forever.
                if p.current == before {
                    p.bump();
                }
            }
            if !parsed_any_arm {
                p.error_unexpected_token("at least one catch arm".to_string());
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
        // `while let PATTERN = SCRUTINEE { ... }` is a distinct refutable form.
        // Decided here by peeking past `while` for a `let` token — patterns
        // always start with `let` (BAML's binding marker), and no normal
        // expression starts with `let`, so this is unambiguous. Mirrors
        // `parse_if_expr`'s `if` vs `if let` decision.
        if self.peek_is_binding_intro_stmt(1) {
            self.parse_while_let_stmt();
            return;
        }

        self.with_node(SyntaxKind::WHILE_STMT, |p| {
            p.expect(TokenKind::While);

            // Condition: same destructure suppression as `if` — see
            // `parse_if_expr` for rationale.
            p.suppress_destructure_pattern_depth += 1;
            p.suppress_object_literal_depth += 1;
            p.parse_expr();
            p.suppress_object_literal_depth -= 1;
            p.suppress_destructure_pattern_depth -= 1;

            // Body
            if p.at(TokenKind::LBrace) {
                p.parse_block_expr();
            } else {
                p.error_unexpected_token("block after while condition".to_string());
            }
        });
    }

    /// Parse `while let PATTERN = SCRUTINEE { ... }`.
    ///
    /// Caller (`parse_while_stmt`) has already verified the sequence starts
    /// with `while let`. Mirrors `parse_if_let_expr` (the `let` is consumed by
    /// `parse_pattern`, BAML patterns carry their own leading `let` binding
    /// marker), but with NO `else` branch — a loop produces unit.
    fn parse_while_let_stmt(&mut self) {
        self.with_node(SyntaxKind::WHILE_LET_STMT, |p| {
            p.expect(TokenKind::While);
            // Pattern. For top-level array patterns (`while let [a, b] = xs`),
            // the `let` keyword has to be consumed at the statement level
            // because `parse_let_pattern` only handles binding / destructure
            // shapes after `let`. Mirrors `parse_if_let_expr`.
            if p.binding_intro_is_followed_by_array_pattern() {
                p.bump_binding_intro(); // statement-level binding introducer
                p.parse_pattern();
            } else {
                // The introducer is consumed inside parse_pattern → parse_let_pattern.
                p.parse_pattern();
            }

            if !p.eat(TokenKind::Equals) {
                p.error_unexpected_token("'=' after while-let pattern".to_string());
            }

            // Scrutinee — exclude assignment operators (same as let-stmt and
            // if-let), and apply condition-position destructure suppression so
            // a trailing `is Class { ... }` doesn't eat the loop body block.
            p.suppress_destructure_pattern_depth += 1;
            p.suppress_object_literal_depth += 1;
            p.parse_expr_bp(3);
            p.suppress_object_literal_depth -= 1;
            p.suppress_destructure_pattern_depth -= 1;

            // Body
            if p.at(TokenKind::LBrace) {
                p.parse_block_expr();
            } else {
                p.error_unexpected_token("block after while-let scrutinee".to_string());
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
                if p.at_binding_intro_stmt() {
                    // Peek ahead to check if this is iterator-style (has 'in' keyword)
                    // For iterator-style: for (let i in expr) / for (const i in expr)
                    // For C-style: for (let i = 0; ...) / for (const i = 0; ...)
                    if p.looks_like_for_in_loop() {
                        // Iterator-style: for (let var in expr) / for (const var in expr)
                        p.parse_for_in_pattern();
                        p.expect(TokenKind::In);
                        let suppress_object_literal = !p.has_for_header_closing_paren_ahead();
                        if suppress_object_literal {
                            p.suppress_object_literal_depth += 1;
                        }
                        p.parse_expr(); // iterator expression
                        if suppress_object_literal {
                            p.suppress_object_literal_depth -= 1;
                        }
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
                p.suppress_object_literal_depth += 1;
                p.allow_object_literal_before_for_body_depth += 1;
                p.parse_expr();
                p.allow_object_literal_before_for_body_depth -= 1;
                p.suppress_object_literal_depth -= 1;
            }

            // Body
            if p.at(TokenKind::LBrace) {
                p.parse_block_expr();
            } else {
                p.error_unexpected_token("block after for expression".to_string());
            }
        });
    }

    /// Whether the current parenthesized for-in header has a closing `)` ahead.
    ///
    /// If it does not, a following `{ ... }` must remain available as the loop
    /// body instead of being consumed as an object literal. Balanced nested
    /// delimiters are skipped so valid iterables such as `Items { values }`,
    /// arrays containing objects, and calls continue to work.
    fn has_for_header_closing_paren_ahead(&self) -> bool {
        let mut stack: Vec<TokenKind> = Vec::new();
        let mut saw_top_level_brace = false;
        let mut i = 0;

        loop {
            let Some(tok) = self.peek(i) else {
                return false;
            };
            match tok.kind {
                TokenKind::LParen => stack.push(TokenKind::RParen),
                TokenKind::LBracket => stack.push(TokenKind::RBracket),
                TokenKind::LBrace => {
                    if stack.is_empty() {
                        saw_top_level_brace = true;
                    }
                    stack.push(TokenKind::RBrace);
                }
                TokenKind::RParen if stack.is_empty() => return true,
                close @ (TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace) => {
                    let Some(expected) = stack.pop() else {
                        return false;
                    };
                    if expected != close {
                        return false;
                    }
                }
                TokenKind::Semicolon if stack.is_empty() => return false,
                TokenKind::For
                | TokenKind::If
                | TokenKind::While
                | TokenKind::Let
                | TokenKind::Return
                | TokenKind::Break
                | TokenKind::Continue
                | TokenKind::Match
                    if stack.is_empty() && saw_top_level_brace =>
                {
                    return false;
                }
                _ => {}
            }
            i += 1;
        }
    }

    /// Check if this looks like a for-in loop. We're at a binding introducer.
    /// Scan forward
    /// past the (possibly complex) pattern that follows: bindings, paths,
    /// destructures (`{ ... }`), parenthesised groups, type annotations
    /// after `:`. Whichever of `in` / `=` / `;` we hit at the top level
    /// (no open brackets/parens/braces) decides the form: `in` →
    /// iterator-style, `=` or `;` → C-style.
    ///
    /// Uses a stack of expected closers so out-of-order delimiters (e.g.
    /// `( [ ) ]`) bail out rather than mis-classify.
    fn looks_like_for_in_loop(&self) -> bool {
        debug_assert!(self.at_binding_intro());
        let mut stack: Vec<TokenKind> = Vec::new();
        let mut i: usize = 1; // start after the binding introducer
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
            // `let`/`const`. No initializer.
            if !p.at_binding_intro_stmt() {
                p.error_unexpected_token("'let'".to_string());
            }
            if p.binding_intro_is_followed_by_array_pattern() {
                p.bump_binding_intro(); // statement-level binding introducer
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
        loop {
            // `current()` skips all line comments, including headers. Stop on a raw header before
            // looking up the next operator so the surrounding block or arm loop can emit it as a
            // structural `HEADER_COMMENT` node rather than `bump()` consuming it as trivia.
            if self.at_header_comment_start() {
                break;
            }

            let Some(token) = self.current() else {
                break;
            };
            let op = token.kind;

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
            } else if op == TokenKind::LParen && !self.newline_separates_block_expr_from_paren() {
                // Function call.
                //
                // The `newline_separates_block_expr_from_paren()` guard fixes a
                // guard-style early return (B-622): a block-terminated
                // statement whose value is discarded, followed on the *next
                // line* by a parenthesized expression, must not glue into a
                // call on that value. For example
                //   if (x < 0) { throw ... }
                //   (x * 2)
                // would otherwise parse as `{ ... }(x * 2)`, invoking the void
                // `if` result (E0006 "`void` is not a function"). Only the
                // block-terminated + newline shape is separated (mirroring
                // Rust's expression-statement rule); ordinary calls whose `(`
                // sits on a later line than a non-`}` callee — e.g. a method
                // chain broken across lines by comments — still parse as calls.
                let lhs_start = self.find_previous_expr_start_after(expr_start);
                self.wrap_events_in_node(lhs_start, SyntaxKind::CALL_EXPR);
                self.parse_call_args();
                self.finish_node();
            } else if op == TokenKind::Backtick && !self.has_newline_ahead() {
                // Tagged template: `tag` ` … ` ` lowers at HIR time to a call
                // where the body becomes a lambda producing `TaggedString`
                // (BEP-049 §10). Recognised as a postfix on any expression so
                // `sql`…`` parses; further restrictions (the target must be a
                // function marked `//baml:tagged_string`) are enforced later.
                //
                // We require no newline between the tag expression and the
                // backtick — otherwise statement-terminating layouts like
                //   let name = "world"
                //   `Hello, ${name}!`
                // would wrongly absorb the standalone backtick literal as a
                // postfix on `"world"`. JS uses the same `no-LineTerminator`
                // restriction here for the same reason.
                let lhs_start = self.find_previous_expr_start_after(expr_start);
                self.wrap_events_in_node(lhs_start, SyntaxKind::TAGGED_TEMPLATE_EXPR);
                self.parse_backtick_string();
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
                    if self.at_member_name() {
                        self.bump();
                    } else {
                        self.error_unexpected_token(
                            "field name, `[`, or `(` after `?.`".to_string(),
                        );
                    }
                    self.finish_node();
                }
            } else if op == TokenKind::At
                && !self.has_newline_ahead()
                && self
                    .peek(1)
                    .is_some_and(|t| t.kind == TokenKind::Word && t.text == "spec")
            {
                // Postfix `@spec` on an LLM function reference: `MyFunc@spec(...)`.
                // Wraps the base expression in a SPEC_EXPR; AST lowering renames
                // the path's last segment to the `<name>$spec` companion. The
                // no-newline guard mirrors the tagged-template rule so an
                // attribute at the start of the next line is never absorbed.
                let lhs_start = self.find_previous_expr_start_after(expr_start);
                self.wrap_events_in_node(lhs_start, SyntaxKind::SPEC_EXPR);
                self.bump(); // @
                self.bump(); // spec
                self.finish_node();
            } else if op == TokenKind::Dot && self.looks_like_as_projection() {
                let lhs_start = self.find_previous_expr_start_after(expr_start);
                self.wrap_events_in_node(lhs_start, SyntaxKind::UPCAST_EXPR);
                self.bump(); // .
                self.bump_contextual_kw_as("as", SyntaxKind::KW_AS);
                self.parse_generic_args();
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
                let lhs_start = self.find_previous_expr_start_after(expr_start);
                self.wrap_events_in_node(lhs_start, SyntaxKind::FIELD_ACCESS_EXPR);
                self.bump(); // . or $
                if self.at_member_name() {
                    self.bump();
                } else {
                    let punct = if op == TokenKind::Dollar {
                        "'$'"
                    } else {
                        "'.'"
                    };
                    self.error_unexpected_token(format!("field name after `{punct}`"));
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
                let object_literal_allowed = self.suppress_object_literal_depth == 0
                    || (self.allow_object_literal_before_for_body_depth > 0
                        && self.object_literal_can_precede_for_body());
                if object_literal_allowed
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
            } else if op == TokenKind::Not && !self.has_newline_ahead() {
                // Postfix `!` (TypeScript/Swift/Kotlin non-null assertion).
                // BAML has no such operator: `!` is only a *prefix* unary
                // operator (handled in `parse_prefix`). Reaching it here means
                // the preceding expression is complete and `!` is dangling in
                // postfix position (e.g. `xs.at(0)!`).
                //
                // We must consume it with a targeted diagnostic. Leaving it
                // unconsumed lets the statement loop re-enter `parse_prefix`,
                // which would treat the `!` as a *prefix* operator applied to
                // whatever follows (a `}` or `else`), producing a misleading
                // "expected expression"/"if without else" error cascade that
                // never points at the `!` itself.
                //
                // The `!self.has_newline_ahead()` guard is essential: BAML
                // separates statements by newlines (no semicolons), so a `!`
                // at the *start of the next line* is a legitimate prefix unary
                // operator on a fresh statement, e.g.
                //   let v: int | string = "hi"
                //   !(v is int)
                // Without the guard the trailing-`!` branch would greedily
                // absorb that prefix `!` as a postfix on the previous
                // statement's value. This mirrors the same no-newline
                // restriction the tagged-template (`Backtick`) branch uses.
                let bang_span = token.span;
                self.error(
                    format!(
                        "unexpected '!'; BAML has no non-null assertion operator — \
                         unwrap optionals with '?? <default>' or '{IF_LET_UNWRAP_SHAPE}'"
                    ),
                    bang_span,
                );
                self.bump(); // consume the stray '!'
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
    /// `<word> :`, a shorthand field (`<word> ,` / `<word> }`), or `...`
    /// (spread), which indicates an object literal / constructor.
    /// If it starts with something else (e.g. a statement, keyword, or expression),
    /// the `{` is more likely a block body.
    fn brace_content_looks_like_fields(&self) -> bool {
        // peek() already skips trivia (whitespace, newlines, comments),
        // so peek(1) is the first content token after `{`.
        match self.peek(1) {
            Some(t) if t.kind == TokenKind::DotDotDot => true, // spread
            Some(t) if t.kind == TokenKind::RBrace => true,    // empty braces
            // `client` is a keyword but a valid field name (BEP-049 §10
            // `Context { client: ... }`), so an object literal can begin with it.
            Some(t) if t.kind == TokenKind::Word || t.kind == TokenKind::Client => {
                let mut i = 2;
                while self.peek(i).map(|t| t.kind) == Some(TokenKind::Dot)
                    && self.peek(i + 1).map(|t| t.kind) == Some(TokenKind::Word)
                {
                    i += 2;
                }
                self.peek(i).is_some_and(|t| {
                    t.kind == TokenKind::Colon
                        || (i == 2 && matches!(t.kind, TokenKind::Comma | TokenKind::RBrace))
                })
            }
            _ => false,
        }
    }

    /// Whether the current `{ ... }` can be an object literal inside an
    /// unparenthesized for-in iterable without stealing the loop body.
    ///
    /// A matching brace followed by postfix continuation (`.field`, indexing,
    /// a call, etc.) is still part of the iterable. A second `{` is the
    /// unambiguous `Constructor { fields } { body }` shape.
    fn object_literal_can_precede_for_body(&self) -> bool {
        debug_assert!(self.at(TokenKind::LBrace));

        let mut stack = vec![TokenKind::RBrace];
        let mut i = 1;
        loop {
            let Some(token) = self.peek(i) else {
                return false;
            };
            match token.kind {
                TokenKind::LParen => stack.push(TokenKind::RParen),
                TokenKind::LBracket => stack.push(TokenKind::RBracket),
                TokenKind::LBrace => stack.push(TokenKind::RBrace),
                close @ (TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace) => {
                    let Some(expected) = stack.pop() else {
                        return false;
                    };
                    if close != expected {
                        return false;
                    }
                    if stack.is_empty() {
                        return self.peek(i + 1).is_some_and(|next| {
                            matches!(
                                next.kind,
                                TokenKind::LBrace
                                    | TokenKind::Dot
                                    | TokenKind::Dollar
                                    | TokenKind::QuestionDot
                                    | TokenKind::LBracket
                                    | TokenKind::LParen
                            )
                        });
                    }
                }
                _ => {}
            }
            i += 1;
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
            // awaited value. The operand is parsed *no-catch* (like the
            // payload of `throw`, see `parse_throw_expr`) so a trailing
            // `catch` binds to the whole `await expr` — i.e.
            // `await f catch (e) {…}` is `(await f) catch (e) {…}`, catching
            // the error `await` re-throws — not `await (f catch …)`, which
            // would attach the handler to the never-throwing future handle.
            self.with_node(SyntaxKind::AWAIT_EXPR, |p| {
                p.bump(); // `await`
                p.parse_expr_bp_no_catch(PREFIX_BP);
            });
        } else if self.at(TokenKind::Spawn) {
            // BEP-034 `spawn name_expr? block`.
            self.parse_spawn_expr();
        } else {
            self.parse_primary_expr();
        }
    }

    /// Parse `spawn name_expr? (with expr (, expr)*)? { body }`. The name
    /// expression is optional and is parsed until we see `with` or `{`. The
    /// optional `with` clause (BEP-034 spawn options) is a comma-separated
    /// list of expressions; in v1 the only accepted form is a single
    /// `baml.spawn.options(...)` call, enforced later in TIR. The body is
    /// always a brace-delimited block.
    fn parse_spawn_expr(&mut self) {
        self.with_node(SyntaxKind::SPAWN_EXPR, |p| {
            p.bump(); // `spawn`
            // Optional name expression: anything that can lead an
            // expression and is not `{` or `with`. We parse the name with a
            // binding power of 1 (`with` is a bare Word with no infix binding
            // power, so the name naturally terminates before it), then the
            // optional `with` clause, then the brace-block.
            if !p.at(TokenKind::LBrace) && !p.at_contextual_kw("with") {
                // Suppress object-literal postfix so the body brace
                // isn't consumed as a struct constructor — without
                // this, `spawn nm { y: 1 }` parses `nm { y: 1 }` as
                // an OBJECT_LITERAL and the body is missing.
                p.suppress_object_literal_depth += 1;
                p.parse_expr_bp(1);
                p.suppress_object_literal_depth -= 1;
            }
            // Optional `with expr (, expr)*` clause. Suppress object literals
            // for the same reason as the name: keep the body brace available.
            if p.at_contextual_kw("with") {
                p.bump_contextual_with();
                p.suppress_object_literal_depth += 1;
                p.parse_expr_bp(1);
                while p.eat(TokenKind::Comma) {
                    // Tolerate a trailing comma before the body brace.
                    if p.at(TokenKind::LBrace) {
                        break;
                    }
                    p.parse_expr_bp(1);
                }
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
        if self.at(TokenKind::BigintLiteral)
            || self.at(TokenKind::IntegerLiteral)
            || self.at(TokenKind::FloatLiteral)
        {
            // Numeric literal
            self.bump();
        } else if self.parse_any_string() {
            // String literal
        } else if self.at(TokenKind::Throw) {
            // Throw expression
            self.parse_throw_expr();
        } else if self.at(TokenKind::Return) {
            // Return expression (diverging, type `never`) — lets `return` be a
            // `catch`/`match` arm value. Statement-position `return` is taken by
            // `parse_stmt` before reaching here.
            self.parse_return_expr();
        } else if self.at(TokenKind::Break) {
            // Break expression (diverging, type `never`) — lets `break` be a
            // `catch`/`match` arm value, symmetric with `return`.
            // Statement-position `break` is taken by `parse_stmt` first.
            self.parse_break_expr();
        } else if self.at(TokenKind::Continue) {
            // Continue expression (diverging, type `never`) — the `continue`
            // counterpart of the `break` case above.
            self.parse_continue_expr();
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
            } else if text == "true" {
                self.bump_contextual_kw_as("true", SyntaxKind::KW_TRUE);
            } else if text == "false" {
                self.bump_contextual_kw_as("false", SyntaxKind::KW_FALSE);
            } else if text == "null" {
                self.bump_contextual_kw_as("null", SyntaxKind::KW_NULL);
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
            // Parenthesized expression. Parens reset the destructure and
            // object-literal suppression: `if (x is Foo { f })` and
            // `if (Config { enabled })` let the user opt back into syntax that
            // condition position would otherwise confuse with the body block.
            self.with_node(SyntaxKind::PAREN_EXPR, |p| {
                p.bump(); // (
                let saved_destructure = std::mem::take(&mut p.suppress_destructure_pattern_depth);
                let saved_object = std::mem::take(&mut p.suppress_object_literal_depth);
                p.parse_expr();
                p.suppress_destructure_pattern_depth = saved_destructure;
                p.suppress_object_literal_depth = saved_object;
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
                        return self.generic_args_follow_at_peek(i);
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
                        return self.generic_args_follow_at_peek(i);
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
                // - `BigintLiteral` / `IntegerLiteral` / `FloatLiteral` for
                //   literal-union members
                // - `Minus` to allow negative numeric literal types (`-1`)
                //   that `parse_type_primary` accepts as type atoms
                // - `Quote` / `Hash` for string-literal types (`"a"`,
                //   `#"raw"#`)
                // - `LParen` / `RParen` for parenthesized union types
                //   (`(int | string)`)
                TokenKind::Word
                | TokenKind::Dot
                | TokenKind::Comma
                | TokenKind::Equals
                | TokenKind::LBracket
                | TokenKind::RBracket
                | TokenKind::Question
                | TokenKind::Pipe
                | TokenKind::BigintLiteral
                | TokenKind::IntegerLiteral
                | TokenKind::FloatLiteral
                | TokenKind::Minus
                | TokenKind::Quote
                | TokenKind::Hash
                | TokenKind::LParen
                | TokenKind::RParen
                // `spawn`/`await` are valid namespace segments inside type
                // args (`foo<baml.spawn.SpawnParams<T, E>>(x)`), mirroring
                // the type-path parser's segment set.
                | TokenKind::Spawn
                | TokenKind::Await => {}
                // Anything else — operators, braces, EOF-ish tokens — can't
                // appear in a type, so this `<` is a comparison.
                _ => return false,
            }
            i += 1;
        }
    }

    /// Decide whether a balanced `<...>` is a generic-argument list (vs. a `<`
    /// comparison) based on the token that follows the closing `>`.
    ///
    /// This ports TypeScript's `canFollowTypeArgumentsInExpression`
    /// (typescript-go internal/parser/parser.go) so that generic callables can
    /// be referenced as values (`let f = foo<int>;`), not only called
    /// (`foo<int>(x)`). `preceded_by_newline` is whether a line break sits
    /// between the closing `>` and `follow`.
    fn is_generic_args_follow(follow: Option<TokenKind>, preceded_by_newline: bool) -> bool {
        use TokenKind::{
            Dot, Greater, GreaterGreater, LBrace, LParen, Less, LessLess, Minus, Plus,
        };
        match follow {
            // Definitely type args: a call `(`, a generic constructor
            // `Box<int> { ... }`, or a qualified path `Wrapper<T>.of(x)`.
            Some(LParen | LBrace | Dot) => true,
            // Ambiguous with comparison/shift — favor the comparison reading.
            // Mirrors TS's false-set (`<`, `>`, `+`, `-`); `+`/`-` here are
            // unary, and `<<`/`>>` are BAML's compound shift tokens.
            Some(Less | LessLess | Greater | GreaterGreater | Plus | Minus) => false,
            // End of input cannot start an expression → favor type args.
            None => true,
            // TS fallback: favor the type-argument interpretation when the
            // closing `>` is followed by a line break, a binary operator, or
            // anything that cannot start an expression.
            Some(kind) => {
                preceded_by_newline
                    || Self::is_binary_operator(kind)
                    || !Self::is_start_of_expression(kind)
            }
        }
    }

    /// Follow-check for the peek-based scan: `gt_peek` is the peek index of the
    /// closing `>`/`>>` token; the follow token is `peek(gt_peek + 1)`.
    fn generic_args_follow_at_peek(&self, gt_peek: usize) -> bool {
        let follow = self.peek(gt_peek + 1).map(|t| t.kind);
        let preceded_by_newline = self
            .raw_index_of_peek(gt_peek)
            .is_some_and(|gt_raw| self.newline_before_next_non_trivia(gt_raw + 1));
        Self::is_generic_args_follow(follow, preceded_by_newline)
    }

    /// Whether `kind` is an infix/binary operator (mirrors TS `isBinaryOperator`).
    fn is_binary_operator(kind: TokenKind) -> bool {
        Self::infix_binding_power(kind).is_some()
    }

    /// Whether `kind` can begin an expression (mirrors TS `isStartOfExpression`).
    /// Covers the primary-expression starts of `parse_primary_expr` and the
    /// unary/prefix starts of `parse_prefix`.
    fn is_start_of_expression(kind: TokenKind) -> bool {
        use TokenKind::{
            Await, BigintLiteral, Client, FloatLiteral, Hash, If, IntegerLiteral, LBrace, LBracket,
            LParen, Less, Match, Minus, MinusMinus, Not, PlusPlus, Quote, Spawn, Throw, Tilde,
            Word,
        };
        matches!(
            kind,
            // primary expression starts
            BigintLiteral
                | IntegerLiteral
                | FloatLiteral
                | Quote   // string literal
                | Hash    // raw string literal `#"..."#`
                | Word
                | Client
                | LParen
                | LBracket
                | LBrace
                | If
                | Match
                | Throw
                | Less    // generic lambda `<T>(...)`
                // unary / prefix expression starts
                | Minus
                | Not
                | Tilde
                | PlusPlus
                | MinusMinus
                | Await
                | Spawn
        )
    }

    /// Raw token index of the `n`-th non-trivia token at/after the cursor,
    /// mirroring `peek`'s comment/whitespace skipping. Used to recover line-break
    /// information that `peek` discards.
    fn raw_index_of_peek(&self, n: usize) -> Option<usize> {
        let mut count = 0;
        let mut i = self.current;
        while i < self.tokens.len() {
            let new_i = self.skip_comment_at(i);
            if new_i != i {
                i = new_i;
                continue;
            }
            if !self.is_basic_trivia(self.tokens[i].kind) {
                if count == n {
                    return Some(i);
                }
                count += 1;
            }
            i += 1;
        }
        None
    }

    /// Whether a `Newline` token sits between raw index `after` and the next
    /// non-trivia token (comments count as trivia).
    fn newline_before_next_non_trivia(&self, after: usize) -> bool {
        let mut i = after;
        while i < self.tokens.len() {
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

    /// Parse type arguments in a type context: `<Type, Assoc = Type, ...>`.
    fn parse_type_args(&mut self) {
        self.type_args_depth += 1;
        self.with_node(SyntaxKind::TYPE_ARGS, |p| {
            p.expect(TokenKind::Less);

            if !p.at(TokenKind::Greater) && !p.at(TokenKind::GreaterGreater) {
                p.parse_type_arg_or_associated_binding();

                while p.pending_greaters == 0 && p.eat(TokenKind::Comma) {
                    if p.at(TokenKind::Greater) || p.at(TokenKind::GreaterGreater) {
                        break; // Trailing comma
                    }
                    p.parse_type_arg_or_associated_binding();
                }
            } else {
                p.error_unexpected_token("type".to_string());
            }

            p.expect_greater();
        });
        self.type_args_depth -= 1;

        // If we just exited the outermost generic and have pending '>', report error.
        if self.type_args_depth == 0 && self.pending_greaters > 0 {
            if let Some(span) = self.pending_greater_span {
                self.error(
                    format!(
                        "unmatched `>` in type expression (found {} extra)",
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
                        "unmatched `>` in generic argument list (found {} extra)",
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
    /// Maps start with { "string":, { identifier:, or a shorthand property
    /// such as { identifier } / { identifier, ... }.
    /// Blocks typically start with { keyword or { expression (but not field:value pattern)
    fn looks_like_map(&self) -> bool {
        // Must start with {
        if !self.at(TokenKind::LBrace) {
            return false;
        }

        // A structural header immediately inside braces makes this an executable block. Normal
        // lookahead skips `//#` as line-comment trivia, so inspect the raw stream before peeking at
        // the first expression; otherwise `{ //# section\n "value" }` is mistaken for a map.
        let mut raw = self.skip_trivia_and_comments_from(self.current);
        if self.tokens.get(raw).map(|token| token.kind) == Some(TokenKind::LBrace) {
            raw += 1;
            loop {
                while self
                    .tokens
                    .get(raw)
                    .is_some_and(|token| self.is_basic_trivia(token.kind))
                {
                    raw += 1;
                }
                if self.is_header_comment_at(raw) {
                    return false;
                }
                let after_comment = self.skip_comment_at(raw);
                if after_comment == raw {
                    break;
                }
                raw = after_comment;
            }
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

                // Check if word or qualified word path is followed by colon.
                // Config-style (word value) is only allowed in config contexts, not expressions
                let mut i = 2;
                while self.peek(i).map(|t| t.kind) == Some(TokenKind::Dot)
                    && self.peek(i + 1).map(|t| t.kind) == Some(TokenKind::Word)
                {
                    i += 2;
                }
                if self.peek(i).map(|t| t.kind) == Some(TokenKind::Colon) {
                    return true; // word: pattern indicates a map
                }
                if i == 2
                    && matches!(
                        self.peek(i).map(|t| t.kind),
                        Some(TokenKind::Comma | TokenKind::RBrace)
                    )
                {
                    return true; // bare word followed by ',' / '}' is shorthand
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
    /// Colons are required. Commas are canonical but may be omitted when the
    /// following token unambiguously starts another key.
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
                        if !p.eat(TokenKind::Comma)
                            && !p.at(TokenKind::Word)
                            && !p.at(TokenKind::Quote)
                            && !p.at(TokenKind::Hash)
                            && !p.at(TokenKind::RBrace)
                        {
                            p.error_unexpected_token("',' or '}' after map entry".to_string());
                            p.bump();
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

        // `spawn` / `await` are reserved keywords but are valid as path
        // segments after a `.` (e.g. the `baml.spawn` namespace). They are
        // unambiguous in segment position — a leading `spawn`/`await` is
        // handled before this point. `is_ident_token` in
        // `baml_compiler2_ast::lower_expr_body` must mirror this set.
        let segment = |k: TokenKind| {
            matches!(
                k,
                TokenKind::Word | TokenKind::Client | TokenKind::Spawn | TokenKind::Await
            )
        };

        // Check if this looks like a path (ident.client followed by dot and another ident)
        if self.peek(1).map(|t| t.kind) == Some(TokenKind::Dot)
            && self.peek(2).map(|t| segment(t.kind)).unwrap_or(false)
            && !(self
                .peek(2)
                .is_some_and(|t| t.kind == TokenKind::Word && t.text == "as")
                && self.peek(3).map(|t| t.kind) == Some(TokenKind::Less))
        {
            // It's a path - all segments are identifiers
            self.with_node(SyntaxKind::PATH_EXPR, |p| {
                p.bump(); // First segment

                // Parse remaining segments
                while p.at(TokenKind::Dot) && !p.looks_like_as_projection() {
                    p.bump();
                    if p.current().map(|t| segment(t.kind)).unwrap_or(false) {
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

    /// Parse a single map entry in expression context: `key: value` or the
    /// shorthand `key`, which desugars to `key: key` during AST lowering.
    fn parse_map_entry(&mut self) {
        self.with_node(SyntaxKind::OBJECT_FIELD, |p| {
            // Key - can be identifier, qualified identifier, or string literal.
            let mut shorthand_candidate = false;
            if p.at(TokenKind::Word) {
                shorthand_candidate = true;
                p.bump(); // identifier key
                while p.at(TokenKind::Dot) {
                    shorthand_candidate = false;
                    p.bump();
                    if !p.expect(TokenKind::Word) {
                        return;
                    }
                }
            } else if !p.parse_any_string() {
                p.error_unexpected_token("map key".to_string());
                return;
            }

            if shorthand_candidate && (p.at(TokenKind::Comma) || p.at(TokenKind::RBrace)) {
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
            // Check for valid field start (`client` is a keyword but a valid
            // field name — BEP-049 §10 `Context { client: ... }`).
            } else if self.at(TokenKind::Word)
                || self.at(TokenKind::Client)
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

    /// Parse a single object field: `name: value` or shorthand `name`.
    fn parse_object_field(&mut self) {
        self.with_node(SyntaxKind::OBJECT_FIELD, |p| {
            // Field name - can be identifier, qualified identifier, or string literal.
            // `client` is a keyword but a valid field name (BEP-049 §10
            // `Context { client: ... }`).
            let mut shorthand_candidate = false;
            if p.at(TokenKind::Word) || p.at(TokenKind::Client) {
                shorthand_candidate = true;
                p.bump(); // identifier field name
                while p.at(TokenKind::Dot) {
                    shorthand_candidate = false;
                    p.bump();
                    if !p.expect(TokenKind::Word) {
                        return;
                    }
                }
            } else if !p.parse_any_string() {
                p.error_unexpected_token("field name".to_string());
                return;
            }

            if shorthand_candidate && (p.at(TokenKind::Comma) || p.at(TokenKind::RBrace)) {
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

    /// Parse a client declaration.
    ///
    /// Two forms: `client Name = <expr>;` (a named client value — the
    /// single-path world) and the legacy `client<llm> Name { ... }` config
    /// block, which still parses so lowering can emit a targeted migration
    /// error instead of a parse cascade.
    pub(crate) fn parse_client(&mut self) {
        if self.peek(1).map(|t| t.kind) == Some(TokenKind::Word)
            && self.peek(2).map(|t| t.kind) == Some(TokenKind::Equals)
        {
            self.with_node(SyntaxKind::CLIENT_VALUE_DEF, |p| {
                p.expect(TokenKind::Client);
                p.bump(); // name
                p.bump(); // =
                p.parse_expr();
                p.eat(TokenKind::Semicolon);
            });
            return;
        }
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
        if self.at(TokenKind::BigintLiteral)
            || self.at(TokenKind::IntegerLiteral)
            || self.at(TokenKind::FloatLiteral)
        {
            return true;
        }
        if self.at(TokenKind::Minus)
            && self.peek(1).is_some_and(|t| {
                matches!(
                    t.kind,
                    TokenKind::BigintLiteral | TokenKind::IntegerLiteral | TokenKind::FloatLiteral
                )
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

            // Same rule as `parse_testset`: a bare identifier as a
            // top-level test name can never resolve — report the syntax
            // fix instead of a downstream "unresolved name".
            if p.testset_body_depth == 0
                && p.at(TokenKind::Word)
                && (p.peek(1).map(|t| t.kind) == Some(TokenKind::LBrace)
                    || Self::token_is_contextual_kw(p.peek(1), "with"))
            {
                let span = p.current().map(|t| t.span).unwrap();
                let name = p.current().map(|t| t.text.clone()).unwrap_or_default();
                p.error(
                    format!("test names must be quoted strings: `test \"{name}\"`"),
                    span,
                );
            }

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

            // A bare identifier as a top-level testset name can never
            // resolve (there are no local bindings at top level), and the
            // downstream "unresolved name" error is misleading. Catch the
            // syntax mistake here with the actual fix. Inside a testset
            // body (depth > 0) identifiers stay legal: names may be
            // computed from loop variables.
            if p.testset_body_depth == 0
                && p.at(TokenKind::Word)
                && (p.peek(1).map(|t| t.kind) == Some(TokenKind::LBrace)
                    || Self::token_is_contextual_kw(p.peek(1), "with"))
            {
                let span = p.current().map(|t| t.span).unwrap();
                let name = p.current().map(|t| t.text.clone()).unwrap_or_default();
                p.error(
                    format!("testset names must be quoted strings: `testset \"{name}\"`"),
                    span,
                );
            }

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

    /// Consume a deprecated top-level `generator NAME { … }` block **without
    /// parsing its interior**. Code generators are now configured in
    /// `baml.toml` under `[generator.<name>]`; the body here is swallowed as
    /// opaque tokens so stale config never produces interior parse errors, and
    /// CST → AST lowering raises a migration warning when it sees the
    /// `GENERATOR_DEF` node (see `lower_cst`).
    pub(crate) fn parse_generator(&mut self) {
        self.with_node(SyntaxKind::GENERATOR_DEF, |p| {
            // 'generator' keyword
            p.expect(TokenKind::Generator);

            // Optional generator name — kept as the first WORD child so
            // lowering can name it in the diagnostic.
            if p.at(TokenKind::Word) {
                p.bump();
            }

            // Swallow the `{ … }` body opaquely, tracking brace depth so
            // nested braces (e.g. option maps) don't terminate early. The
            // interior is deliberately NOT parsed into config items.
            if p.at(TokenKind::LBrace) {
                p.bump(); // consume '{'
                let mut depth = 1usize;
                while depth > 0 && !p.at_end() {
                    if p.at(TokenKind::LBrace) {
                        depth += 1;
                    } else if p.at(TokenKind::RBrace) {
                        depth -= 1;
                    }
                    p.bump();
                }
                // Reached EOF with the body still open — preserve the
                // unclosed-brace diagnostic rather than silently swallowing
                // the rest of the file into this deprecated node.
                if depth > 0 {
                    p.error_unexpected_token("'}'".to_string());
                }
            }
        });
    }

    // ============ Template String Parsing ============

    /// Parse a template string declaration
    pub(crate) fn parse_template_string(&mut self) {
        self.with_node(SyntaxKind::TEMPLATE_STRING_DEF, |p| {
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
                p.bump_contextual_kw_as("type", SyntaxKind::KW_TYPE);
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
        if parser.consume_function_header_comment_if_allowed() {
            continue;
        }

        let attributed_item = if parser.at(TokenKind::AtAt) {
            parser.item_keyword_after_leading_block_attributes()
        } else {
            None
        };

        if parser.at(TokenKind::Enum) || attributed_item == Some(TokenKind::Enum) {
            parser.parse_enum();
        } else if parser.at(TokenKind::Class) || attributed_item == Some(TokenKind::Class) {
            parser.parse_class();
        } else if parser.at(TokenKind::Interface) || attributed_item == Some(TokenKind::Interface) {
            parser.parse_interface();
        } else if parser.at(TokenKind::Function) || attributed_item == Some(TokenKind::Function) {
            parser.parse_function();
        } else if parser.at(TokenKind::Implements) || parser.at(TokenKind::Implement) {
            parser.parse_implements_for();
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
        } else if parser.at_binding_intro_stmt() {
            parser.parse_let_stmt();
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
        if parser.at_line_comment_start() {
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
    use baml_compiler_syntax::{Item, SourceFile, SyntaxKind, SyntaxNode};
    use rowan::ast::AstNode;

    use super::{IF_LET_UNWRAP_SHAPE, ParseError, parse_file};

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

    /// A run of hashes at EOF (an incomplete raw string like `##`) must
    /// parse to errors, never panic: `find_token_after_hashes` returns
    /// `None` at EOF instead of the one-past-the-end index its callers
    /// would use to index `tokens`.
    #[test]
    fn bare_hashes_at_eof_do_not_panic() {
        for source in [
            "##",
            "#",
            "function main() -> string {\n  ##",
            "function main() -> string {\n  ## ",
            "let x = ##",
            "client<llm> C { key ##",
            "client<llm> C {\n  provider openai\n  key ##",
        ] {
            // Parsing must not index past the token buffer (and must not
            // loop: config-value recovery leaves the hashes unconsumed, and
            // the next config-item iteration consumes them as a malformed
            // key). All of these inputs are malformed, so they must also
            // surface diagnostics rather than silently parse.
            let (_root, errors) = parse_source(source);
            assert!(
                !errors.is_empty(),
                "expected at least one diagnostic for {source:?}"
            );
        }
    }

    /// A leading `#!` shebang line parses as a comment, so the file behind
    /// it is valid BAML — this is what makes `.baml` files executable via
    /// `#!/usr/bin/env -S baml run --file`.
    #[test]
    fn leading_shebang_is_treated_as_a_line_comment() {
        let source =
            "#!/usr/bin/env -S baml run --file\nfunction main() -> string {\n  \"hi\"\n}\n";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        // The shebang is emitted as a LINE_COMMENT trivia token, and the
        // function after it is parsed normally.
        let has_line_comment = root.descendants_with_tokens().any(|elem| {
            matches!(
                elem,
                rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::LINE_COMMENT
                    && t.text().starts_with("#!")
            )
        });
        assert!(has_line_comment, "shebang should lex as a LINE_COMMENT");
        assert_eq!(
            SourceFile::cast(root)
                .map(|sf| sf.items().count())
                .unwrap_or(0),
            1,
            "the function after the shebang should still parse"
        );
    }

    /// Parsing must stay lossless: the original source — shebang included —
    /// reconstructs exactly from the syntax tree. If `baml fmt` ever dropped
    /// or moved the shebang, the file would stop being executable.
    #[test]
    fn shebang_round_trips_losslessly() {
        let source = "#!/usr/bin/env -S baml run --file\nfunction main() -> string { \"hi\" }\n";
        let (root, _errors) = parse_source(source);
        assert_eq!(root.text().to_string(), source);
    }

    /// `#!` is only a comment delimiter outside string bodies — a `#!`
    /// inside a regular string stays literal, never swallowing the rest of
    /// the line.
    #[test]
    fn hash_bang_inside_string_is_not_a_comment() {
        let source = "function main() -> string {\n  \"a #! b\"\n}\n";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        let has_line_comment = root.descendants_with_tokens().any(|elem| {
            matches!(
                elem,
                rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::LINE_COMMENT
            )
        });
        assert!(
            !has_line_comment,
            "`#!` inside a string must not be treated as a comment"
        );
    }

    #[test]
    fn const_bindings_parse_as_existing_binding_shapes() {
        let source = r#"
function Demo(user: User, xs: int[], value: int | string, items: int[]) -> int {
  const x = 1;
  const y: int = x;
  const _ = y;
  const User { name } = user;
  const [first] = xs;
  if const narrowed: int = value {
    narrowed
  } else {
    0
  };
  while const loop_value: int = value {
    break;
  }
  for (const i = 0; i < 3; i += 1) {
    x += i;
  }
  for (const item in items) {
    x += item;
  }
  for const item in items {
    x += item;
  }
  x
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let const_keywords = root
            .descendants_with_tokens()
            .filter(|elem| {
                matches!(
                    elem,
                    rowan::NodeOrToken::Token(token) if token.kind() == SyntaxKind::KW_CONST
                )
            })
            .count();
        assert_eq!(const_keywords, 10);

        assert!(
            root.descendants()
                .any(|node| node.kind() == SyntaxKind::IF_LET_EXPR),
            "`if const` should keep using IF_LET_EXPR"
        );
        assert!(
            root.descendants()
                .any(|node| node.kind() == SyntaxKind::WHILE_LET_STMT),
            "`while const` should keep using WHILE_LET_STMT"
        );
        assert!(
            root.descendants()
                .any(|node| node.kind() == SyntaxKind::WILDCARD_PATTERN),
            "`const _` should keep using WILDCARD_PATTERN"
        );
        assert!(
            root.descendants()
                .any(|node| node.kind() == SyntaxKind::DESTRUCTURE_PATTERN),
            "`const User {{ ... }}` should keep using DESTRUCTURE_PATTERN"
        );
        assert!(
            root.descendants()
                .any(|node| node.kind() == SyntaxKind::ARRAY_PATTERN),
            "`const [x]` should keep using ARRAY_PATTERN"
        );
    }

    #[test]
    fn is_class_in_condition_does_not_eat_then_block() {
        // Regression: `if x is Class { body }` used to be parsed as
        // `if x is Class { body }` where `{ body }` was the destructure
        // pattern's body — the if-block was eaten. We follow Rust's
        // approach for struct literals in condition position: in
        // `if`/`while` conditions, `Path {` is parsed as just the path,
        // leaving `{` for the outer block.
        let source = r#"
function f(x: int | string) -> string {
  if x is string {
    "str"
  } else {
    "other"
  }
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        // The IF_EXPR should have a BLOCK_EXPR child (the then-block).
        let if_expr = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::IF_EXPR)
            .expect("expected IF_EXPR node");
        let block_count = if_expr
            .children()
            .filter(|n| n.kind() == SyntaxKind::BLOCK_EXPR)
            .count();
        assert_eq!(
            block_count, 2,
            "IF_EXPR should have two BLOCK_EXPR children (then + else); destructure must not consume the then-block"
        );
    }

    #[test]
    fn is_class_destructure_in_condition_with_user_class() {
        // Same as above but with a user class, which is the case the
        // chained if-let test originally tripped over.
        let source = r#"
class Empty {}

function f(r: int | Empty) -> string {
  if r is Empty {
    "empty"
  } else {
    "other"
  }
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let if_expr = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::IF_EXPR)
            .expect("expected IF_EXPR node");
        let block_count = if_expr
            .children()
            .filter(|n| n.kind() == SyntaxKind::BLOCK_EXPR)
            .count();
        assert_eq!(block_count, 2);

        // The IS_EXPR's pattern is just the type `Empty`, no destructure.
        let dest_count = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::DESTRUCTURE_PATTERN)
            .count();
        assert_eq!(
            dest_count, 0,
            "no destructure pattern should be produced in condition position"
        );
    }

    #[test]
    fn is_class_destructure_with_parens_in_condition_still_works() {
        // Escape hatch: parens reset the destructure suppression. The
        // user can still write a destructure inside `is` by wrapping the
        // whole pattern in parens (even though `is`-bindings don't
        // escape, the parser must not reject this form).
        let source = r#"
class User { name string }

function f(r: int | User) -> string {
  if (r is User { name }) {
    "user"
  } else {
    "other"
  }
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        let dest_count = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::DESTRUCTURE_PATTERN)
            .count();
        assert_eq!(
            dest_count, 1,
            "parens-wrapped destructure in condition should still parse as DESTRUCTURE_PATTERN"
        );
    }

    #[test]
    fn if_let_scrutinee_suppresses_destructure() {
        // Regression (CodeRabbit on #3579): the if-let scrutinee is in
        // condition position; a trailing `is Class { ... }` in the
        // scrutinee must not consume the then-block as a destructure
        // pattern body.
        let source = r#"
class Empty {}

function f(b: bool, r: int | Empty) -> string {
  if let v: bool = r is Empty {
    "yes"
  } else {
    "no"
  }
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        // The if-let must have two BLOCK_EXPR children (then + else); no
        // DESTRUCTURE_PATTERN should be produced.
        let if_let = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::IF_LET_EXPR)
            .expect("expected IF_LET_EXPR");
        let block_count = if_let
            .children()
            .filter(|n| n.kind() == SyntaxKind::BLOCK_EXPR)
            .count();
        assert_eq!(
            block_count, 2,
            "IF_LET_EXPR should have two BLOCK_EXPR children (scrutinee's `is Empty {{ ... }}` must not eat the then-block)"
        );
        let dest_count = if_let
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::DESTRUCTURE_PATTERN)
            .count();
        assert_eq!(dest_count, 0);
    }

    #[test]
    fn if_let_accepts_top_level_array_pattern() {
        // Regression (CodeRabbit on #3579): `if let [a, b] = xs { ... }`
        // used to error because `parse_let_pattern` demanded an identifier
        // after `let`. Mirror the let-stmt handling — when `let` is
        // followed by `[`, consume the keyword and parse an array pattern.
        let source = r#"
function f(xs: int[]) -> int {
  if let [a, b] = xs {
    a + b
  } else {
    0
  }
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        // We should have exactly one IF_LET_EXPR with an ARRAY_PATTERN
        // somewhere inside it (under the PATTERN wrapper).
        let if_let = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::IF_LET_EXPR)
            .expect("expected IF_LET_EXPR");
        let array_count = if_let
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::ARRAY_PATTERN)
            .count();
        assert_eq!(
            array_count, 1,
            "expected exactly one ARRAY_PATTERN in the if-let"
        );
    }

    /// The postfix-`!` diagnostic points users at two ways to unwrap an
    /// optional. Instantiate the metavariables in the `if let` form it quotes
    /// and parse the result, so the hint cannot drift back to a shape the
    /// grammar doesn't have — B-1129 quoted `if (let x) = opt`, which yielded
    /// four fresh errors the moment anyone pasted it.
    #[test]
    fn optional_unwrap_hint_suggests_real_syntax() {
        let concrete = IF_LET_UNWRAP_SHAPE
            .replace(": T ", ": int ")
            .replace("{ ... }", "{ x }");
        let source = format!("function f(opt: int?) -> int {{\n  {concrete} else {{ 0 }}\n}}\n");
        let (root, errors) = parse_source(&source);
        assert_no_errors(&errors);
        // A shape that parses but isn't an `if let` would be just as wrong.
        assert!(
            root.descendants()
                .any(|n| n.kind() == SyntaxKind::IF_LET_EXPR),
            "hint should describe an `if let`, got: {concrete}"
        );
    }

    #[test]
    fn if_let_expr_parses() {
        // `if let PATTERN = SCRUTINEE { ... } else { ... }` produces an
        // IF_LET_EXPR node (distinct from IF_EXPR), with PATTERN as a child.
        let source = r#"
function f(r: int | string) -> string {
  if let v: int = r {
    "int"
  } else {
    "other"
  }
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let if_let_count = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::IF_LET_EXPR)
            .count();
        assert_eq!(if_let_count, 1, "expected one IF_LET_EXPR node");

        // No plain IF_EXPR should sneak in.
        let if_count = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::IF_EXPR)
            .count();
        assert_eq!(if_count, 0, "expected no IF_EXPR nodes for if-let form");

        // The IF_LET_EXPR should have a PATTERN child.
        let if_let = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::IF_LET_EXPR)
            .unwrap();
        let pat_count = if_let
            .children()
            .filter(|n| n.kind() == SyntaxKind::PATTERN)
            .count();
        assert_eq!(
            pat_count, 1,
            "IF_LET_EXPR should have exactly one PATTERN child"
        );
    }

    #[test]
    fn while_let_stmt_parses() {
        // `while let PATTERN = SCRUTINEE { ... }` produces a WHILE_LET_STMT
        // node (distinct from WHILE_STMT), with a PATTERN child and a single
        // BLOCK_EXPR body (no else).
        let source = r#"
function f(r: int | string) -> int {
  while let v: int = r {
    break;
  }
  0
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let while_let_count = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::WHILE_LET_STMT)
            .count();
        assert_eq!(while_let_count, 1, "expected one WHILE_LET_STMT node");

        // No plain WHILE_STMT should sneak in.
        let while_count = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::WHILE_STMT)
            .count();
        assert_eq!(
            while_count, 0,
            "expected no WHILE_STMT nodes for while-let form"
        );

        let while_let = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::WHILE_LET_STMT)
            .unwrap();
        let pat_count = while_let
            .children()
            .filter(|n| n.kind() == SyntaxKind::PATTERN)
            .count();
        assert_eq!(
            pat_count, 1,
            "WHILE_LET_STMT should have exactly one PATTERN child"
        );
        let block_count = while_let
            .children()
            .filter(|n| n.kind() == SyntaxKind::BLOCK_EXPR)
            .count();
        assert_eq!(
            block_count, 1,
            "WHILE_LET_STMT should have exactly one BLOCK_EXPR body (no else)"
        );
    }

    #[test]
    fn while_let_scrutinee_suppresses_destructure() {
        // The while-let scrutinee is in condition position; a trailing
        // `is Class { ... }` must not consume the loop body as a destructure
        // pattern body. Mirrors `if_let_scrutinee_suppresses_destructure`.
        let source = r#"
class Empty {}

function f(b: bool, r: int | Empty) -> int {
  while let v: bool = r is Empty {
    break;
  }
  0
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let while_let = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::WHILE_LET_STMT)
            .expect("expected WHILE_LET_STMT");
        let block_count = while_let
            .children()
            .filter(|n| n.kind() == SyntaxKind::BLOCK_EXPR)
            .count();
        assert_eq!(block_count, 1);
        let dest_count = while_let
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::DESTRUCTURE_PATTERN)
            .count();
        assert_eq!(dest_count, 0);
    }

    #[test]
    fn while_let_accepts_top_level_array_pattern() {
        // `while let [a, b] = xs { ... }` — the `let` followed by `[` must be
        // consumed at statement level so the array pattern parses. Mirrors
        // `if_let_accepts_top_level_array_pattern`.
        let source = r#"
function f(xs: int[]) -> int {
  while let [a, b] = xs {
    break;
  }
  0
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let while_let = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::WHILE_LET_STMT)
            .expect("expected WHILE_LET_STMT");
        let array_count = while_let
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::ARRAY_PATTERN)
            .count();
        assert_eq!(
            array_count, 1,
            "expected exactly one ARRAY_PATTERN in the while-let"
        );
    }

    #[test]
    fn while_stmt_still_parses_without_pattern() {
        // A plain `while cond { }` still produces a WHILE_STMT, not a
        // WHILE_LET_STMT — the `let` lookahead must not misfire.
        let source = r#"
function f(b: bool) -> int {
  while b {
    break;
  }
  0
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let while_count = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::WHILE_STMT)
            .count();
        assert_eq!(while_count, 1, "expected one WHILE_STMT node");
        let while_let_count = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::WHILE_LET_STMT)
            .count();
        assert_eq!(
            while_let_count, 0,
            "plain while must not produce WHILE_LET_STMT"
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
    fn as_projection_parses_in_local_rooted_chain() {
        let source = r#"
function f(i: Item) -> string {
  i.as<Named>.name
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let upcast_count = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::UPCAST_EXPR)
            .count();
        assert_eq!(upcast_count, 1, "expected one UPCAST_EXPR");
    }

    #[test]
    fn nested_generic_bound_closes_function_generic_list() {
        let source = r#"
interface Converter<T> {
  function convert(self) -> T
}

function read_int<T extends Converter<int>>(m: T) -> int {
  m.as<Converter<int>>.convert()
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let generic_param_lists = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::GENERIC_PARAM_LIST)
            .count();
        assert_eq!(
            generic_param_lists, 2,
            "expected interface and function generic params"
        );
    }

    #[test]
    fn extra_generic_parameter_closer_uses_parameter_list_wording() {
        let (_root, errors) = parse_source("function f<T>>() -> int { 1 }");
        assert!(
            errors.iter().any(|error| matches!(
                error,
                ParseError::InvalidSyntax { message, .. }
                    if message == "unmatched `>` in generic parameter list (found 1 extra)"
            )),
            "expected generic parameter list wording, got: {errors:#?}"
        );
    }

    #[test]
    fn extra_generic_argument_closer_uses_argument_list_wording() {
        let (_root, errors) = parse_source("function f(x: int) -> int { x.as<int>>() }");
        assert!(
            errors.iter().any(|error| matches!(
                error,
                ParseError::InvalidSyntax { message, .. }
                    if message == "unmatched `>` in generic argument list (found 1 extra)"
            )),
            "expected generic argument list wording, got: {errors:#?}"
        );
    }

    #[test]
    fn generic_bound_alias_syntax_is_not_supported() {
        let source = r#"
interface Converter<T> {
  function convert(self) -> T
}

function read_int<T extends Converter<int> as Ints>(m: T) -> int {
  return m.as<Converter<int>>.convert()
}
"#;
        let (_root, errors) = parse_source(source);
        assert!(
            errors.iter().any(|error| matches!(
                error,
                ParseError::InvalidSyntax { message, .. }
                    if message.contains("generic bound aliases are not supported")
            )),
            "expected alias syntax to be rejected at the generic bound, got: {errors:#?}"
        );
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
    fn source_file_items_include_interfaces_and_out_of_body_implements() {
        let source = r#"
interface Named {
  name string
}

implements Named for int {
  function name(self) -> string {
    "int"
  }
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let source_file = SourceFile::cast(root).expect("expected SOURCE_FILE");
        let item_kinds: Vec<_> = source_file
            .items()
            .map(|item| match item {
                Item::Interface(_) => "interface",
                Item::ImplementsFor(_) => "implements_for",
                other => panic!("unexpected item: {other:?}"),
            })
            .collect();

        assert_eq!(item_kinds, vec!["interface", "implements_for"]);
    }

    #[test]
    fn parses_interface_method_with_leading_block_attributes() {
        let source = r#"
interface Response {
  @@internal.throws(NetworkError)
  function text(self) -> string
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let method = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::METHOD_SIG)
            .expect("expected METHOD_SIG");
        let attrs: Vec<_> = method
            .children()
            .filter(|n| n.kind() == SyntaxKind::BLOCK_ATTRIBUTE)
            .collect();

        assert_eq!(attrs.len(), 1, "expected interface method block attribute");
    }

    #[test]
    fn header_comments_are_diagnosed_outside_expression_blocks() {
        let source = r#"
//# top-level declaration
interface Response {
  //# interface members
  function text(self) -> string
}

enum Status {
  //# enum members
  Ready //# trailing enum header
}

client<llm> TestClient {
  //# config items
  provider openai
  options {
    //# nested config items
    model "gpt-4o"
  }
}

test Legacy {
  //# test config
  functions []
  type_builder {
    //# type builder items
    class Built {
      //# class members
      value string
    }
  }
}

function llm_body() -> string {
  client TestClient
  //# llm fields
  prompt `hello`
}

function executable() -> int {
  //# executable section
  1
}
"#;

        let (root, errors) = parse_source(source);

        assert_eq!(
            errors.len(),
            10,
            "every non-expression header should produce one diagnostic: {errors:#?}"
        );
        assert!(errors.iter().all(|error| {
            format!("{error:?}")
                .contains("header comments (`//#`) are only allowed in expression functions")
        }));

        assert_eq!(
            root.descendants()
                .filter(|node| node.kind() == SyntaxKind::HEADER_COMMENT)
                .count(),
            1,
            "only the executable-block header should become a syntax node"
        );
        assert_eq!(
            root.descendants_with_tokens()
                .filter(|elem| {
                    matches!(
                        elem,
                        rowan::NodeOrToken::Token(token)
                            if token.kind() == SyntaxKind::LINE_COMMENT
                                && token.text().starts_with("//#")
                    )
                })
                .count(),
            10,
            "non-expression headers should be ordinary line-comment trivia"
        );
    }

    #[test]
    fn function_headers_are_allowed_only_before_expression_functions() {
        let source = r#"
//# top-level expression function with a shift default
function top_level(x: int = 8 >> 1) -> int {
  1
}

//# top-level LLM function
function llm() -> string {
  client "openai/gpt-4o"
  prompt `hello`
}

class Methods {
  //# expression method
  function value(self) -> int {
    1
  }

  //# LLM method
  function generated(self) -> string {
    client "openai/gpt-4o"
    prompt `hello`
  }
}

interface DefaultMethods {
  //# expression default method with a shift default
  function implements(self, x: int = 8 >> 1) -> int {
    1
  }

  //# required method
  function required(self) -> int
}
"#;

        let (root, errors) = parse_source(source);

        assert_eq!(
            errors.len(),
            3,
            "headers before LLM functions and required signatures should be rejected: {errors:#?}"
        );
        assert!(errors.iter().all(|error| {
            format!("{error:?}")
                .contains("header comments (`//#`) are only allowed in expression functions")
        }));
        assert_eq!(
            root.descendants()
                .filter(|node| node.kind() == SyntaxKind::HEADER_COMMENT)
                .count(),
            0,
            "function-level headers remain line-comment trivia"
        );
        assert_eq!(
            root.descendants_with_tokens()
                .filter(|elem| matches!(
                    elem,
                    rowan::NodeOrToken::Token(token)
                        if token.kind() == SyntaxKind::LINE_COMMENT
                            && token.text().starts_with("//#")
                ))
                .count(),
            6,
            "allowed and rejected function-level headers should remain line-comment trivia"
        );
    }

    #[test]
    fn header_led_nested_block_is_not_a_map_literal() {
        let source = r#"
function nested() -> string {
  {
    //# section
    "value"
  }
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        assert!(
            root.descendants()
                .any(|node| node.kind() == SyntaxKind::HEADER_COMMENT),
            "the nested expression block should preserve its structural header"
        );
        assert!(
            root.descendants()
                .all(|node| node.kind() != SyntaxKind::MAP_LITERAL),
            "the header-led nested block must not be parsed as a map literal"
        );
    }

    #[test]
    fn header_comments_are_structural_inside_expression_contexts() {
        let source = r#"
function sound() -> string {
  //# statement position
  "woof"
}

function classify(n: int) -> string {
  match (n) {
    //# leading header before the first arm
    0 => "zero",
    //# header between arms
    _ => "other",
  }
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        assert_eq!(
            root.descendants()
                .filter(|node| node.kind() == SyntaxKind::HEADER_COMMENT)
                .count(),
            3,
            "all expression-context headers should become structural nodes"
        );
        assert!(
            root.descendants_with_tokens().all(|elem| !matches!(
                elem,
                rowan::NodeOrToken::Token(token)
                    if token.kind() == SyntaxKind::LINE_COMMENT
                        && token.text().starts_with("//#")
            )),
            "expression-context headers must not be reduced to line-comment trivia"
        );
    }

    #[test]
    fn parses_interface_default_method_body_after_comments() {
        let source = r#"
interface Response {
  function text(self) -> string
    // This comment mentions } but should not end the method.
    /* This one mentions { and also should be ignored. */
  {
    "ok"
  }
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let interface = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::INTERFACE_DEF)
            .expect("expected INTERFACE_DEF");
        assert!(
            interface
                .children()
                .any(|n| n.kind() == SyntaxKind::FUNCTION_DEF),
            "default interface method should parse as FUNCTION_DEF"
        );
        assert!(
            !interface
                .children()
                .any(|n| n.kind() == SyntaxKind::METHOD_SIG),
            "commented default method must not parse as a required signature"
        );
    }

    #[test]
    fn interface_keyword_recovers_as_top_level_after_missing_class_brace() {
        let source = r#"
class Person {
  name string

interface Named {
  name string
}
"#;

        let (root, errors) = parse_source(source);
        assert!(
            !errors.is_empty(),
            "missing class brace should still produce a parse error"
        );

        assert!(
            root.children()
                .any(|n| n.kind() == SyntaxKind::INTERFACE_DEF),
            "interface should recover as a top-level item, not be swallowed by the class"
        );
    }

    #[test]
    fn multiline_interface_keyword_recovers_as_top_level_after_missing_class_brace() {
        let source = r#"
class Person {
  name string

interface Named
requires Base
{
  name string
}
"#;

        let (root, errors) = parse_source(source);
        assert!(
            !errors.is_empty(),
            "missing class brace should still produce a parse error"
        );

        assert!(
            root.children()
                .any(|n| n.kind() == SyntaxKind::INTERFACE_DEF),
            "multiline interface should recover as a top-level item"
        );
    }

    #[test]
    fn interface_keyword_can_be_class_field_name() {
        let source = r#"
class InterfaceTwo {
  interface strin
}
"#;

        let (root, _errors) = parse_source(source);
        let class = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::CLASS_DEF)
            .expect("expected CLASS_DEF");
        assert!(
            class.children().any(|n| n.kind() == SyntaxKind::FIELD),
            "`interface strin` should parse as a class field, not as a recovered top-level interface"
        );
        assert!(
            !root
                .children()
                .any(|n| n.kind() == SyntaxKind::INTERFACE_DEF),
            "field named `interface` must not create a top-level interface"
        );
    }

    #[test]
    fn client_keyword_can_be_class_field_name() {
        // BEP-049 §10: `Context` has a `client` field (`ctx.client`), but
        // `client` is also a top-level keyword (`client<llm> Name { … }`).
        // A `client Type` field must NOT trigger missing-brace recovery.
        let source = r#"
class Ctx {
  client string
  tags string
}
"#;
        let (root, errors) = parse_source(source);
        assert!(
            errors.is_empty(),
            "`client` field should parse cleanly: {errors:?}"
        );
        let class = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::CLASS_DEF)
            .expect("expected CLASS_DEF");
        assert_eq!(
            class
                .children()
                .filter(|n| n.kind() == SyntaxKind::FIELD)
                .count(),
            2,
            "both `client` and `tags` should parse as fields"
        );
    }

    #[test]
    fn client_keyword_is_valid_member_access() {
        // `ctx.client` must parse — `client` stays valid as a member name.
        let source = r#"
function f(ctx: Context) -> string {
  ctx.client.provider
}
"#;
        let (_root, errors) = parse_source(source);
        assert!(
            errors.is_empty(),
            "`ctx.client.provider` member access should parse cleanly: {errors:?}"
        );
    }

    #[test]
    fn client_declaration_still_recovers_inside_class() {
        // The `client<…>` declaration form must STILL trigger missing-brace
        // recovery (it is not a field).
        let source = r#"
class Broken {
  name string
client<llm> Foo {
  provider "openai"
}
"#;
        let (root, errors) = parse_source(source);
        assert!(
            !errors.is_empty(),
            "missing closing brace should produce a parse error"
        );
        assert!(
            root.children().any(|n| n.kind() == SyntaxKind::CLIENT_DEF),
            "client<llm> should recover as a top-level declaration"
        );
    }

    #[test]
    fn raw_string_keeps_template_markers_as_text() {
        for marker in ["//", "*/", "{{ name }}", "{% if true %}", "{# note #}"] {
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
    fn parses_template_string_for_lowering_diagnostic() {
        let source = "template_string Greeting(name: string) `Hello ${name}`";
        let (root, errors) = parse_source(source);

        assert_no_errors(&errors);
        assert!(
            root.descendants()
                .any(|node| node.kind() == SyntaxKind::TEMPLATE_STRING_DEF),
            "unsupported declaration should remain in the tree for lowering"
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
    fn expression_body_calling_a_client_param_is_not_llm_body() {
        // `client` lexes as KW_CLIENT, so a call THROUGH a parameter named
        // `client` looks like the start of an LLM directive unless `(` is
        // excluded — the body would then be parsed as an LLM block and fail.
        let source = r#"
function f(client: (int) -> int) -> int {
  client(1)
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
    fn expression_body_call_with_client_named_arg_is_not_llm_body() {
        let source = r#"
function main() -> string {
  Ask("hi", client = override_provider())
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
    fn expression_body_call_with_prompt_named_arg_is_not_llm_body() {
        let source = r#"
function main() -> string {
  render(prompt = "hello", client = c)
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
    fn single_line_llm_body_parses_client_and_prompt_fields() {
        // B-621: a single-line LLM body with `client` and `prompt` on the same
        // line must not swallow `prompt` into the unquoted client value and then
        // misreport it as missing. Both fields must be recognized, error-free.
        let source = "function F(raw: string) -> C { client: Fast prompt: `hi` }\n";

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let llm_body = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::LLM_FUNCTION_BODY)
            .expect("expected an LLM_FUNCTION_BODY, not an expression body");

        let client_field = llm_body
            .children()
            .find(|n| n.kind() == SyntaxKind::CLIENT_FIELD)
            .expect("expected a CLIENT_FIELD");
        assert!(
            client_field.text().to_string().contains("Fast"),
            "client field should name `Fast`, got: {}",
            client_field.text()
        );
        // The `prompt` word and its value must land in the PROMPT_FIELD, not the
        // client value.
        assert!(
            !client_field.text().to_string().contains("prompt"),
            "client value must not swallow the `prompt` field: {}",
            client_field.text()
        );

        assert!(
            llm_body
                .children()
                .any(|n| n.kind() == SyntaxKind::PROMPT_FIELD),
            "expected a PROMPT_FIELD in the single-line body"
        );
    }

    #[test]
    fn llm_body_bare_client_does_not_swallow_next_line_prompt_field() {
        // B-621 regression guard: a bare `client` with no value on its line must
        // NOT absorb the `prompt` field that starts on the following line into the
        // client value. The client value stays empty and the PROMPT_FIELD is parsed.
        let source = "function F(raw: string) -> C {\n  client\n  prompt #\"hi\"#\n}\n";

        let (root, _errors) = parse_source(source);
        let llm_body = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::LLM_FUNCTION_BODY)
            .expect("expected an LLM_FUNCTION_BODY");

        let client_field = llm_body
            .children()
            .find(|n| n.kind() == SyntaxKind::CLIENT_FIELD)
            .expect("expected a CLIENT_FIELD");
        assert!(
            !client_field.text().to_string().contains("prompt"),
            "bare `client` must not swallow the next-line `prompt` field: {}",
            client_field.text()
        );
        assert!(
            llm_body
                .children()
                .any(|n| n.kind() == SyntaxKind::PROMPT_FIELD),
            "expected the `prompt` field to be parsed as its own PROMPT_FIELD"
        );
    }

    #[test]
    fn llm_body_client_scan_terminates_on_truncated_and_eof_inputs() {
        // B-621 hang guard: the unquoted client-value scan must make forward
        // progress and terminate for every input, including bodies truncated at
        // EOF with no closing brace, newline, or next-field-start to break on.
        // If the scan can spin, `parse_source` never returns and this test hangs
        // (surfacing as a timeout instead of relying on wasm-pack to catch it).
        let inputs = [
            "function F() -> C { client",
            "function F() -> C { client:",
            "function F() -> C { client Fast",
            "function F() -> C { client: Fast",
            "function F() -> C { client Fast prompt",
            "function F() -> C { client Fast prompt:",
            "function F() -> C { client Fast, ",
            "function F() -> C { client Fast; ",
            "function F() -> C { client Fast prompt: `hi`",
            "function F() -> C { client openai/gpt-4o-mini",
            "function F() -> C { client openai/gpt-4o-mini prompt",
            "function F() -> C { client Fast //trailing",
            "function F() -> C { client Fast /*unterminated",
            "function F() -> C { client Fast prompt `hi",
            "function F() -> C {\n  client",
            "function F() -> C {\n  client\n  prompt",
            "function F() -> C {\n  client\n",
        ];
        for input in inputs {
            // The assertion is simply that this call returns.
            let _ = parse_source(input);
        }
    }

    #[test]
    fn single_line_llm_body_comma_separator_reports_targeted_error() {
        // B-621: `,` between fields is not a valid separator. The diagnostic must
        // name the real requirement (a newline) rather than falsely claiming the
        // prompt is missing.
        let source = "function F(raw: string) -> C { client: Fast, prompt: `hi` }\n";

        let (_root, errors) = parse_source(source);
        assert!(
            errors.iter().any(|error| matches!(
                error,
                ParseError::InvalidSyntax { message, .. }
                    if message.contains("separate `client` and `prompt` with a newline")
            )),
            "expected a targeted separator diagnostic, got: {errors:#?}"
        );
        assert!(
            !errors
                .iter()
                .any(|error| format!("{error:?}").contains("missing 'prompt'")),
            "the misleading missing-prompt error must not fire: {errors:#?}"
        );
    }

    #[test]
    fn single_line_llm_body_semicolon_separator_reports_targeted_error() {
        // B-621: same as the comma case, for `;`.
        let source = "function F(raw: string) -> C { client: Fast; prompt: `hi` }\n";

        let (_root, errors) = parse_source(source);
        assert!(
            errors.iter().any(|error| matches!(
                error,
                ParseError::InvalidSyntax { message, .. }
                    if message.contains("separate `client` and `prompt` with a newline")
            )),
            "expected a targeted separator diagnostic, got: {errors:#?}"
        );
        assert!(
            !errors
                .iter()
                .any(|error| format!("{error:?}").contains("missing 'prompt'")),
            "the misleading missing-prompt error must not fire: {errors:#?}"
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
    fn parses_break_and_continue_as_match_arm_expressions() {
        // B-619: bare `break`/`continue` are valid in match-arm expression
        // position (symmetric with `return`), producing BREAK_EXPR/CONTINUE_EXPR
        // nodes — no `E0010 Expected expression` error.
        let source = r#"
function Demo(n: int) -> int {
  let x = n;
  while (true) {
    match (x) {
      0 => break,
      1 => continue,
      _ => { x = x - 1; }
    }
  }
  x
}
"#;

        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let break_expr_count = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::BREAK_EXPR)
            .count();
        assert_eq!(break_expr_count, 1, "expected one BREAK_EXPR node");

        let continue_expr_count = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::CONTINUE_EXPR)
            .count();
        assert_eq!(continue_expr_count, 1, "expected one CONTINUE_EXPR node");

        // Bare `break`/`continue` in statement position still parse as the
        // statement forms — the expression forms only fire through expression
        // parsing (the match arms above).
        assert_eq!(
            root.descendants()
                .filter(|n| n.kind() == SyntaxKind::BREAK_STMT)
                .count(),
            0,
            "arm-position break should not be a BREAK_STMT"
        );
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

    /// `catch_all_panics` is a contextual keyword: the lexer emits a plain
    /// `Word`, and the parser must still recognize it in catch-clause position
    /// (B-504 — it previously only handled `catch`/`catch_all`).
    #[test]
    fn parses_catch_all_panics_clause() {
        let source = r#"
function Demo() -> int {
  throw 1 catch_all_panics (e) {
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

        // The `Word` is re-labelled as a dedicated keyword token inside the clause.
        let has_kw = catch_expr.descendants_with_tokens().any(|elem| {
            matches!(
                elem,
                rowan::NodeOrToken::Token(t)
                    if t.kind() == SyntaxKind::KW_CATCH_ALL_PANICS && t.text() == "catch_all_panics"
            )
        });
        assert!(has_kw, "expected a KW_CATCH_ALL_PANICS token in the clause");
    }

    /// `catch_all_panics` is contextual, not reserved: outside catch-clause
    /// position it is still a perfectly good identifier (e.g. a function name).
    #[test]
    fn catch_all_panics_is_a_normal_identifier_outside_catch() {
        let source = r#"
function catch_all_panics() -> int { 1 }

function Demo() -> int {
  let x = catch_all_panics()
  x
}
"#;

        let (_root, errors) = parse_source(source);
        assert_no_errors(&errors);
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

    /// Helper: does the parse tree contain a `GENERIC_ARGS` node?
    fn has_generic_args(root: &SyntaxNode) -> bool {
        root.descendants()
            .any(|n| n.kind() == SyntaxKind::GENERIC_ARGS)
    }

    #[test]
    fn bare_generic_instantiation_in_value_position() {
        // `let f = foo<int>;` — a generic function referenced with explicit type
        // args but NOT called. Ported from TS instantiation expressions: the
        // closing `>` followed by `;` (which cannot start an expression) must
        // disambiguate as GENERIC_ARGS, not `foo < int` comparison.
        let source = r#"
function Demo() -> int {
  let f = foo<int>;
  1
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        assert!(
            has_generic_args(&root),
            "expected GENERIC_ARGS for bare `foo<int>` in value position"
        );
    }

    #[test]
    fn bare_generic_instantiation_followed_by_terminators() {
        // Closing `>` followed by `,`, `)`, `]`, `}` (none can start an
        // expression) → all GENERIC_ARGS.
        for source in [
            "function Demo() -> int { let xs = [foo<int>, bar<string>]; 1 }",
            "function Demo() -> int { g(foo<int>); 1 }",
            "function Demo() -> int { foo<int> }",
        ] {
            let (root, errors) = parse_source(source);
            assert_no_errors(&errors);
            assert!(
                has_generic_args(&root),
                "expected GENERIC_ARGS for terminator-followed instantiation in: {source}"
            );
        }
    }

    #[test]
    fn bare_generic_instantiation_followed_by_member_access() {
        // `foo<int>.bar` — `.` is in the true follow-set.
        let source = r#"
function Demo() -> int {
  let x = foo<int>.bar;
  1
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        assert!(
            has_generic_args(&root),
            "expected GENERIC_ARGS for `foo<int>.bar`"
        );
    }

    #[test]
    fn comparison_chain_is_not_generic_args() {
        // `a < b > c` — the token after `>` is `c`, which CAN start an
        // expression and is not a binary operator, so this stays a comparison
        // (TS canFollowTypeArgumentsInExpression fallback → false).
        let source = r#"
function Demo() -> bool {
  let r = a < b > c;
  r
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        assert!(
            !has_generic_args(&root),
            "expected NO GENERIC_ARGS for comparison chain `a < b > c`"
        );
    }

    #[test]
    fn simple_comparison_is_not_generic_args() {
        // `a < b` with no closing `>` fails the type-token scan → comparison.
        let source = r#"
function Demo() -> bool {
  let r = a < b;
  r
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        assert!(
            !has_generic_args(&root),
            "expected NO GENERIC_ARGS for simple comparison `a < b`"
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
    fn lambda_body_requires_braces() {
        for source in [
            "function Demo() -> int { let add_one = (x: int) -> x + 1; add_one(41) }",
            "function Demo() -> int { let add_one = (x: int) => x + 1; add_one(41) }",
        ] {
            let (_root, errors) = parse_source(source);
            assert!(
                errors.iter().any(|error| matches!(
                    error,
                    ParseError::UnexpectedToken {
                        expected,
                        found,
                        ..
                    } if expected == "lambda body '{'" && found == "'+'"
                )),
                "expected a required lambda block-body diagnostic, got: {errors:#?}"
            );
        }
    }

    #[test]
    fn enum_semicolon_delimiter_is_accepted_for_formatter_repair() {
        let source = "enum Status { Pending; Complete; }\n";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        assert_eq!(
            root.descendants()
                .filter(|node| node.kind() == SyntaxKind::ENUM_VARIANT)
                .count(),
            2
        );
    }

    #[test]
    fn map_entries_without_commas_are_accepted_for_formatter_repair() {
        let source = "function Demo() -> int { { \"left\": 1 \"right\": 2 } }\n";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        let map = root
            .descendants()
            .find(|node| node.kind() == SyntaxKind::MAP_LITERAL)
            .expect("expected map literal");
        assert_eq!(
            map.children()
                .filter(|node| node.kind() == SyntaxKind::OBJECT_FIELD)
                .count(),
            2
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

    // ─── BEP-049: backtick string literals ────────────────────────────────────

    fn find_backtick_literal(root: &SyntaxNode) -> SyntaxNode {
        root.descendants()
            .find(|n| n.kind() == SyntaxKind::BACKTICK_STRING_LITERAL)
            .expect("expected BACKTICK_STRING_LITERAL node")
    }

    #[test]
    fn match_scrutinee_parens_optional() {
        // Host parser inconsistency: `if`/`while` accept parens-optional
        // conditions, but `match` previously required parens. Bring it in
        // line with the other control-flow forms.
        let source = "
function Demo(x: int) -> int {
    match x {
        1 => 10,
        _ => 0,
    }
}
";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        let m = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::MATCH_EXPR)
            .expect("expected MATCH_EXPR");
        assert!(m.text().to_string().contains("match x {"));
    }

    #[test]
    fn match_scrutinee_with_type_annotation_no_parens() {
        // `match (expr : Type)` was the only place the `: Type` annotation
        // worked. With paren-optional, ensure the annotation still works
        // when parens ARE present.
        let source = "
function Demo(x: int) -> int {
    match (x : int) {
        _ => 0,
    }
}
";
        let (_root, errors) = parse_source(source);
        assert_no_errors(&errors);
    }

    #[test]
    fn match_paren_form_still_works() {
        let source = "
function Demo(x: int) -> int {
    match (x) {
        1 => 10,
        _ => 0,
    }
}
";
        let (_root, errors) = parse_source(source);
        assert_no_errors(&errors);
    }

    #[test]
    fn backtick_m3_for_open_emits_for_node() {
        let source = "
function Demo(xs: int[]) -> string {
    `${for (let x in xs)}- ${x}
${endfor}`
}
";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        let for_open = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::BACKTICK_FOR_OPEN)
            .expect("expected BACKTICK_FOR_OPEN");
        assert!(for_open.text().to_string().contains("${for"));
        let endfor = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::BACKTICK_ENDFOR)
            .expect("expected BACKTICK_ENDFOR");
        assert_eq!(endfor.text().to_string(), "${endfor}");
    }

    #[test]
    fn backtick_m3_if_block_tag_vs_if_expression() {
        // Same condition shape, different surface form: `}` after cond
        // → block-tag; `{` after cond → if-expression.
        let source = "
function Demo(c: bool) -> string {
    let a = `${if (c)}yes${endif}`
    let b = `${if (c) { \"yes\" } else { \"no\" }}`
    a + b
}
";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        let if_opens: Vec<_> = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::BACKTICK_IF_OPEN)
            .collect();
        assert_eq!(
            if_opens.len(),
            1,
            "expected exactly one BACKTICK_IF_OPEN (block-tag form)"
        );
        let endifs: Vec<_> = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::BACKTICK_ENDIF)
            .collect();
        assert_eq!(endifs.len(), 1);
        let if_exprs: Vec<_> = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::IF_EXPR)
            .collect();
        assert_eq!(
            if_exprs.len(),
            1,
            "expected exactly one IF_EXPR (expression form)"
        );
    }

    #[test]
    fn backtick_m3_if_no_parens_block_tag() {
        // `${if cond}` (no parens) — should still dispatch as block-tag.
        let source = "
function Demo(c: bool) -> string {
    `${if c}yes${endif}`
}
";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        assert!(
            root.descendants()
                .any(|n| n.kind() == SyntaxKind::BACKTICK_IF_OPEN),
            "expected BACKTICK_IF_OPEN for `${{if c}}` (no parens)"
        );
    }

    #[test]
    fn backtick_m3_else_and_else_if() {
        let source = "
function Demo(n: int) -> string {
    `${if (n > 0)}pos${else if (n < 0)}neg${else}zero${endif}`
}
";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        assert!(
            root.descendants()
                .any(|n| n.kind() == SyntaxKind::BACKTICK_IF_OPEN)
        );
        assert!(
            root.descendants()
                .any(|n| n.kind() == SyntaxKind::BACKTICK_ELSE_IF)
        );
        assert!(
            root.descendants()
                .any(|n| n.kind() == SyntaxKind::BACKTICK_ELSE)
        );
        assert!(
            root.descendants()
                .any(|n| n.kind() == SyntaxKind::BACKTICK_ENDIF)
        );
    }

    #[test]
    fn backtick_m4_tagged_template_wraps_backtick() {
        // BEP-049 §10. `name` immediately preceding a backtick parses as
        // a TAGGED_TEMPLATE_EXPR; the inner BACKTICK_STRING_LITERAL is a
        // child of the new node so HIR lowering has both the tag callee
        // and the body in one shape.
        let source = "
function Demo() -> string {
    let q = sql`SELECT * FROM t WHERE id = ${1}`
    q
}
";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        let tagged = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::TAGGED_TEMPLATE_EXPR)
            .expect("expected TAGGED_TEMPLATE_EXPR");
        // Tag identifier sits at the start of the tagged expr's text.
        assert!(tagged.text().to_string().starts_with("sql`"));
        // The wrapper must enclose a BACKTICK_STRING_LITERAL.
        assert!(
            tagged
                .descendants()
                .any(|n| n.kind() == SyntaxKind::BACKTICK_STRING_LITERAL)
        );
    }

    #[test]
    fn backtick_m4_untagged_backtick_stays_unwrapped() {
        // A bare backtick literal — no preceding identifier — should NOT
        // become a TAGGED_TEMPLATE_EXPR. Guards against the postfix branch
        // misfiring on prefix-only expressions.
        let source = "
function Demo() -> string {
    `hello`
}
";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        assert!(
            !root
                .descendants()
                .any(|n| n.kind() == SyntaxKind::TAGGED_TEMPLATE_EXPR),
            "untagged backtick must not wrap in TAGGED_TEMPLATE_EXPR"
        );
        assert!(
            root.descendants()
                .any(|n| n.kind() == SyntaxKind::BACKTICK_STRING_LITERAL)
        );
    }

    #[test]
    fn backtick_basic_one_liner() {
        let source = r#"
function Demo() -> string {
    let s = `hello world`
    s
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        let lit = find_backtick_literal(&root);
        assert_eq!(lit.text().to_string(), "`hello world`");
    }

    #[test]
    fn backtick_multiline() {
        let source = "
function Demo() -> string {
    let s = `
        line one
        line two
    `
    s
}
";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        let lit = find_backtick_literal(&root);
        let text = lit.text().to_string();
        assert!(text.starts_with('`') && text.ends_with('`'));
        assert!(text.contains("line one"));
        assert!(text.contains("line two"));
    }

    #[test]
    fn backtick_multi_tick_ladder_two() {
        // Two-tick delimiter allows a single backtick inside content.
        let source = "
function Demo() -> string {
    let s = ``inline `code` here``
    s
}
";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        let lit = find_backtick_literal(&root);
        assert_eq!(lit.text().to_string(), "``inline `code` here``");
    }

    #[test]
    fn backtick_multi_tick_ladder_three() {
        // Three-tick delimiter allows up to two consecutive backticks in content.
        let source = "
function Demo() -> string {
    let s = ```nested ``double`` ticks```
    s
}
";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        let lit = find_backtick_literal(&root);
        assert_eq!(lit.text().to_string(), "```nested ``double`` ticks```");
    }

    #[test]
    fn backtick_multi_tick_keyword_content_not_flagged_empty() {
        // A top-level item keyword (`function`) sitting mid-line as multi-tick
        // CONTENT must not be treated as an item boundary by the empty-multi-tick
        // guard — the close run is still ahead, so the literal parses normally.
        let source = "
function Demo() -> string {
    let s = ``function keyword``
    s
}
";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        let lit = find_backtick_literal(&root);
        assert_eq!(lit.text().to_string(), "``function keyword``");
    }

    #[test]
    fn backtick_anchored_close_jep326_trailing_extra() {
        // §8 case 3: 3-tick opener with 4 trailing backticks.
        // Anchored-close picks the LAST 3 ticks; one backtick stays in content.
        let source = "
function Demo() -> string {
    let s = ```content````
    s
}
";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        let lit = find_backtick_literal(&root);
        assert_eq!(lit.text().to_string(), "```content````");
    }

    #[test]
    fn backtick_simple_interpolation_emits_interp_node() {
        let source = "
function Demo() -> string {
    let s = `Hello, ${name}!`
    s
}
";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        let interp = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::BACKTICK_INTERPOLATION)
            .expect("expected a BACKTICK_INTERPOLATION node");
        assert_eq!(interp.text().to_string(), "${name}");
    }

    #[test]
    fn backtick_interpolation_with_method_chain() {
        let source = "
function Demo() -> string {
    let s = `${user.name.upper()}`
    s
}
";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        let interp = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::BACKTICK_INTERPOLATION)
            .expect("expected a BACKTICK_INTERPOLATION node");
        assert_eq!(interp.text().to_string(), "${user.name.upper()}");
    }

    #[test]
    fn backtick_interpolation_block_body_with_let_and_tail() {
        // BEP §4: ${...} is a block expression — statements + optional tail.
        let source = "
function Demo() -> string {
    let s = `result: ${ let x = 1; let y = 2; x + y }!`
    s
}
";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        let interp = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::BACKTICK_INTERPOLATION)
            .expect("expected BACKTICK_INTERPOLATION");
        // Body is a nested BLOCK_EXPR (from parse_block_expr).
        let block = interp
            .descendants()
            .find(|n| n.kind() == SyntaxKind::BLOCK_EXPR)
            .expect("expected BLOCK_EXPR inside interpolation");
        let let_count = block
            .children()
            .filter(|n| n.kind() == SyntaxKind::LET_STMT)
            .count();
        assert_eq!(let_count, 2, "expected two let statements in block body");
    }

    #[test]
    fn backtick_interpolation_block_body_no_tail_renders_empty() {
        // Statement-only body is valid; the lowering will render it as "".
        let source = "
function Demo() -> string {
    let s = `set: ${ let x = 1 }!`
    s
}
";
        let (_root, errors) = parse_source(source);
        assert_no_errors(&errors);
    }

    #[test]
    fn backtick_segments_handle_u_f8ff_in_user_content() {
        // Ultrareview bug_006: segments() used U+F8FF (Apple-logo PUA) as
        // an in-band placeholder for interpolations during dedent. If user
        // content also contained U+F8FF and the literal was multi-line
        // with at least one ${...}, split() returned more pieces than
        // expected → the user's U+F8FF was silently dropped and
        // interpolations landed at the wrong positions. Fix: pick a
        // placeholder codepoint that doesn't appear in the joined content.
        use baml_compiler_syntax::{BacktickSegment, BacktickStringLiteral};
        use rowan::ast::AstNode;

        let source = "
function Demo(x: string) -> string {
    let s = `
  before\u{F8FF}between
  ${x}
  after`
    s
}
";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        let lit = BacktickStringLiteral::cast(
            root.descendants()
                .find(|n| n.kind() == SyntaxKind::BACKTICK_STRING_LITERAL)
                .unwrap(),
        )
        .unwrap();
        let segs = lit.segments();
        // The interpolation must remain BETWEEN the "between" and "after"
        // text — not migrated forward — and the user's U+F8FF must survive.
        let text_parts: Vec<&str> = segs
            .iter()
            .filter_map(|s| match s {
                BacktickSegment::Text(t) => Some(t.as_str()),
                BacktickSegment::Interp(_) | BacktickSegment::For(_) | BacktickSegment::If(_) => {
                    None
                }
            })
            .collect();
        let combined = text_parts.join("|");
        assert!(
            combined.contains('\u{F8FF}'),
            "user's U+F8FF was dropped; text parts: {text_parts:?}"
        );
        let interp_count = segs
            .iter()
            .filter(|s| matches!(s, BacktickSegment::Interp(_)))
            .count();
        assert_eq!(interp_count, 1, "expected exactly one Interp segment");
    }

    #[test]
    fn backtick_empty_multi_tick_diagnoses_with_later_real_backtick() {
        // Ultrareview bug_008: `has_backtick_close_ahead_from` walked to
        // EOF, so any unrelated backtick literal later in the file
        // satisfied the look-ahead and disabled the empty-multi-tick
        // guard. Result: the bad opener silently consumed cross-function
        // content, replacing the clean targeted diagnostic with cascading
        // errors.
        //
        // Test uses `` ` `` + word + `` ` `` later to make a 2+ tick
        // adjacent run elsewhere — sufficient to trip the unscoped look-
        // ahead but not a real 2-tick literal.
        let source = "
function Bad() -> string {
    ``
}

function OK() -> string {
    ``twotick``
}
";
        let (_root, errors) = parse_source(source);
        // The diagnostic must point at Bad's `` (around bytes 32-34),
        // NOT at some downstream remnant. If the guard is defeated, Bad's
        // opener silently consumes cross-function content and the empty-
        // multi-tick diagnostic only fires later from leftover backticks.
        let bad_opener_diag = errors.iter().find(|e| {
            let s = format!("{e:?}");
            s.contains("Empty multi-tick backtick string") && {
                // Bad's `` is in the first 50 bytes of source; any
                // diagnostic past that is from a downstream remnant.
                if let Some(range_start) = s.find("range: ") {
                    let tail = &s[range_start + 7..];
                    let end = tail.find('.').unwrap_or(tail.len());
                    tail[..end].parse::<usize>().is_ok_and(|b| b < 50)
                } else {
                    false
                }
            }
        });
        assert!(
            bad_opener_diag.is_some(),
            "expected empty-multi-tick diagnostic AT Bad's `` (bytes <50), got: {errors:#?}"
        );
    }

    #[test]
    fn backtick_empty_multi_tick_diagnoses_with_later_comment_backticks() {
        // Ultrareview bug_002: backticks inside a `// ... ``` ...`
        // line-comment payload also satisfied the look-ahead (the lexer
        // emits them as plain Backtick tokens; only the parser layer
        // assembles `//` into a comment).
        let source = "
function Bad() -> string {
    ``
}

// markdown sample: ```code```
function After() -> string { 1 }
";
        let (_root, errors) = parse_source(source);
        // Same location-based check as bug_008 test: the diagnostic must
        // point at Bad's `` near the top of the file, not at some downstream
        // backtick that the unscoped look-ahead happened to find inside
        // the comment payload.
        let bad_opener_diag = errors.iter().find(|e| {
            let s = format!("{e:?}");
            s.contains("Empty multi-tick backtick string") && {
                if let Some(range_start) = s.find("range: ") {
                    let tail = &s[range_start + 7..];
                    let end = tail.find('.').unwrap_or(tail.len());
                    tail[..end].parse::<usize>().is_ok_and(|b| b < 50)
                } else {
                    false
                }
            }
        });
        assert!(
            bad_opener_diag.is_some(),
            "expected empty-multi-tick diagnostic AT Bad's `` (bytes <50), got: {errors:#?}"
        );
    }

    #[test]
    fn backtick_backslash_followed_by_whitespace_does_not_eat_closer() {
        // Ultrareview bug_011: after a `\\`, the second `bump_raw()` in
        // `parse_backtick_content` skipped trivia before consuming the
        // escape target. So `` `\ ` `` (backslash + space + close) had its
        // closing backtick silently swallowed as the escaped char, and the
        // parser emitted a misleading "Unclosed backtick string" error
        // for a literal that was actually closed.
        let source = "
function Demo() -> string {
    let s = `\\ `
    s
}
";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        let lit = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::BACKTICK_STRING_LITERAL)
            .expect("expected backtick literal");
        // Two backticks total: opener and closer.
        assert_eq!(
            lit.text().to_string().matches('`').count(),
            2,
            "literal should contain opener+closer only, got: {:?}",
            lit.text().to_string()
        );
    }

    #[test]
    fn backtick_after_block_comment_parses() {
        // Regression for PR #3577 review (CodeRabbit r3307652890): the
        // entry guard `self.at(TokenKind::Backtick)` skips comments, so
        // `let s = /*c*/ \`ok\`` reaches `parse_backtick_string`. But the
        // helpers `count_consecutive_backticks` and `find_first_backtick_pos`
        // only skipped *basic* trivia (whitespace + newlines), not comments —
        // returning 0 and failing the parse.
        let source = "
function Demo() -> string {
    let s = /*c*/ `ok`
    s
}
";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        let lit = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::BACKTICK_STRING_LITERAL)
            .expect("expected backtick literal");
        assert_eq!(lit.text().to_string(), "`ok`");
    }

    #[test]
    fn backtick_after_line_comment_parses() {
        let source = "
function Demo() -> string {
    let s = // line comment
        `ok`
    s
}
";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        let lit = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::BACKTICK_STRING_LITERAL)
            .expect("expected backtick literal");
        assert_eq!(lit.text().to_string(), "`ok`");
    }

    #[test]
    fn backtick_segments_multiline_starts_with_interp() {
        // Regression for PR #3577 review (CodeRabbit r3312486624): when the
        // first decoded part is `Interp` and the literal is multi-line (so
        // dedent runs), the remapping loop previously only advanced the
        // split iterator for Text parts. The leading empty piece (for "no
        // Text before first Interp") got assigned to the FIRST Text part,
        // shifting all subsequent Text parts to the wrong piece. The tail-
        // append rescued single-Text cases by accident; multi-Text cases
        // collapsed the wrong pieces together.
        use baml_compiler_syntax::{BacktickSegment, BacktickStringLiteral};
        use rowan::ast::AstNode;

        // Source: backtick body begins with `${a}` (no leading whitespace
        // or newline), followed by multi-line content containing another
        // interp. decoded = [Interp(a), Text("\nhi "), Interp(b), Text(" bye\nend")].
        let source = "
function Demo(a: string, b: string) -> string {
    let s = `${a}
hi ${b} bye
end`
    s
}
";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        let lit = BacktickStringLiteral::cast(
            root.descendants()
                .find(|n| n.kind() == SyntaxKind::BACKTICK_STRING_LITERAL)
                .unwrap(),
        )
        .unwrap();
        let segs = lit.segments();
        let text_parts: Vec<&str> = segs
            .iter()
            .filter_map(|s| match s {
                BacktickSegment::Text(t) => Some(t.as_str()),
                BacktickSegment::Interp(_) | BacktickSegment::For(_) | BacktickSegment::If(_) => {
                    None
                }
            })
            .collect();
        // Each text *between* interpolations should remain at its own
        // segment — not all collapse into the last one.
        assert_eq!(
            text_parts,
            vec!["\nhi ", " bye\nend"],
            "got: {text_parts:?}"
        );
    }

    #[test]
    fn backtick_segments_for_adjacent_interpolations() {
        // Regression: `${a}${b}` previously returned 0 segments because
        // `delimiter_count` used `filter_map(into_token).take_while(BACKTICK)`,
        // which silently skipped past the BACKTICK_INTERPOLATION nodes and
        // miscounted the closing backtick as part of the opener.
        use baml_compiler_syntax::{BacktickSegment, BacktickStringLiteral};
        use rowan::ast::AstNode;

        let source = "
function Demo() -> string {
    let a = \"ab\"
    let b = \"cd\"
    `${a}${b}`
}
";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        let lit = BacktickStringLiteral::cast(
            root.descendants()
                .find(|n| n.kind() == SyntaxKind::BACKTICK_STRING_LITERAL)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(lit.delimiter_count(), 1);
        let segs = lit.segments();
        assert_eq!(segs.len(), 2, "expected two Interp segments, got: {segs:?}");
        assert!(matches!(segs[0], BacktickSegment::Interp(_)));
        assert!(matches!(segs[1], BacktickSegment::Interp(_)));
    }

    #[test]
    fn backtick_interpolation_with_string_literal_inside() {
        // Regression: with the old lexer regex, `before-$` lexed as a single
        // Word token (trailing `$` absorbed), masking the Dollar so
        // interpolation never triggered. Fixed by tightening the Word regex
        // to disallow trailing `$`.
        let source = r#"
function Demo() -> string {
    `before-${"x"}`
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        assert!(
            root.descendants()
                .any(|n| n.kind() == SyntaxKind::BACKTICK_INTERPOLATION),
            "expected BACKTICK_INTERPOLATION node for ${{\"x\"}}"
        );
    }

    #[test]
    fn backtick_segments_split_text_and_interpolation() {
        use baml_compiler_syntax::{BacktickSegment, BacktickStringLiteral};
        use rowan::ast::AstNode;

        let source = "
function Demo() -> string {
    let s = `Hello, ${user.name}!`
    s
}
";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        let lit_node = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::BACKTICK_STRING_LITERAL)
            .expect("expected backtick string literal");
        let lit = BacktickStringLiteral::cast(lit_node).expect("cast");

        let segs = lit.segments();
        assert_eq!(segs.len(), 3, "segments: {segs:?}");
        match &segs[0] {
            BacktickSegment::Text(s) => assert_eq!(s, "Hello, "),
            other => panic!("expected Text, got {other:?}"),
        }
        match &segs[1] {
            BacktickSegment::Interp(node) => {
                assert_eq!(node.text().to_string(), "${user.name}");
            }
            other => panic!("expected Interp, got {other:?}"),
        }
        match &segs[2] {
            BacktickSegment::Text(s) => assert_eq!(s, "!"),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn backtick_segments_multiline_dedent_skips_interp() {
        // BEP §12 rule 8: dedent operates on literal text only; interpolations
        // are treated as opaque inline content and do not affect min-indent.
        use baml_compiler_syntax::{BacktickSegment, BacktickStringLiteral};
        use rowan::ast::AstNode;

        let source = "
function Demo() -> string {
    let s = `
        Hello, ${name}!
        Welcome.
    `
    s
}
";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        let lit = BacktickStringLiteral::cast(
            root.descendants()
                .find(|n| n.kind() == SyntaxKind::BACKTICK_STRING_LITERAL)
                .unwrap(),
        )
        .unwrap();
        let segs = lit.segments();
        // After dedent: leading "Hello, ", interp, "!\nWelcome." (the line
        // delimiter-related line break and indentation before the closing
        // delimiter are removed).
        let text_parts: Vec<&str> = segs
            .iter()
            .filter_map(|s| match s {
                BacktickSegment::Text(t) => Some(t.as_str()),
                BacktickSegment::Interp(_) | BacktickSegment::For(_) | BacktickSegment::If(_) => {
                    None
                }
            })
            .collect();
        assert_eq!(
            text_parts,
            vec!["Hello, ", "!\nWelcome."],
            "got: {text_parts:?}"
        );
    }

    #[test]
    fn backtick_lone_dollar_is_literal_text() {
        // `$` not immediately followed by `{` is content, not interpolation.
        let source = "
function Demo() -> string {
    `cost: $5 and $ {not interp}`
}
";
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        assert!(
            root.descendants()
                .all(|n| n.kind() != SyntaxKind::BACKTICK_INTERPOLATION),
            "lone $ should not emit BACKTICK_INTERPOLATION"
        );
    }

    #[test]
    fn backtick_escaped_dollar_does_not_interpolate() {
        // BEP §8: `\${...}` is literal text inside backticks.
        let source = r#"
function Demo() -> string {
    `literal \${name}`
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        assert!(
            root.descendants()
                .all(|n| n.kind() != SyntaxKind::BACKTICK_INTERPOLATION),
            "escaped \\${{...}} must not emit BACKTICK_INTERPOLATION"
        );
    }

    #[test]
    fn backtick_empty_multi_tick_emits_diagnostic_without_consuming_downstream() {
        // BEP-049 §8 case 1: `` `` `` is an empty 2-tick opener with no close
        // anywhere. The parser must emit a clean diagnostic AT THE OPENER and
        // leave the rest of the file alone — not silently swallow downstream
        // functions while searching for a phantom 2-tick close.
        let source = "
function Empty() -> string {
    ``
}

function After() -> string {
    \"i should still parse\"
}

function MoreCode() -> int {
    1 + 1
}
";
        let (root, errors) = parse_source(source);

        assert!(
            errors
                .iter()
                .any(|e| format!("{e:?}").contains("Empty multi-tick backtick string")),
            "expected an empty-multi-tick parse error, got: {errors:#?}"
        );

        // The degenerate BACKTICK_STRING_LITERAL should contain ONLY the two
        // opener backticks — not the rest of the file.
        let captured: Vec<String> = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::BACKTICK_STRING_LITERAL)
            .map(|n| n.text().to_string())
            .collect();
        assert_eq!(captured, vec!["``".to_string()], "captured: {captured:?}");

        // All three functions should still appear in the parsed tree.
        let fns: Vec<String> = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::FUNCTION_DEF)
            .filter_map(|f| {
                f.children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .find(|t| t.kind() == SyntaxKind::WORD)
                    .map(|t| t.text().to_string())
            })
            .collect();
        assert_eq!(fns, vec!["Empty", "After", "MoreCode"]);
    }

    #[test]
    fn backtick_empty_three_tick_also_diagnosed() {
        // Same rule applies to any N ≥ 2.
        let source = "
function Bad() -> string {
    ```
}
function Good() -> int { 1 }
";
        let (root, errors) = parse_source(source);
        assert!(
            errors
                .iter()
                .any(|e| format!("{e:?}").contains("Empty multi-tick backtick string")),
            "expected empty-multi-tick error for 3-tick opener, got: {errors:#?}"
        );
        let fns: Vec<String> = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::FUNCTION_DEF)
            .filter_map(|f| {
                f.children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .find(|t| t.kind() == SyntaxKind::WORD)
                    .map(|t| t.text().to_string())
            })
            .collect();
        assert_eq!(fns, vec!["Bad", "Good"]);
    }

    #[test]
    fn backtick_unclosed_emits_error() {
        let source = "
function Demo() -> string {
    let s = `not closed
}
";
        let (_root, errors) = parse_source(source);
        assert!(
            errors
                .iter()
                .any(|e| format!("{e:?}").contains("Unclosed backtick string")),
            "expected an Unclosed-backtick parse error, got: {errors:#?}"
        );
    }

    #[test]
    fn let_else_basic_parses() {
        // `let Pattern = scrutinee else { ... };` — refutable binding with a
        // diverging else clause. The LET_STMT should carry both an EQUALS
        // initializer and a trailing KW_ELSE + BLOCK_EXPR.
        let source = r#"
function f(r: int | string) -> int {
  let v: int = r else { return 0; };
  v
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let let_stmt = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::LET_STMT)
            .expect("expected LET_STMT");
        let has_else = let_stmt.children_with_tokens().any(
            |elem| matches!(elem, rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::KW_ELSE),
        );
        assert!(has_else, "LET_STMT should have a KW_ELSE token");
        let block_count = let_stmt
            .children()
            .filter(|n| n.kind() == SyntaxKind::BLOCK_EXPR)
            .count();
        assert_eq!(
            block_count, 1,
            "LET_STMT should have one BLOCK_EXPR (the else body)"
        );
    }

    #[test]
    fn let_else_rejects_else_if() {
        // `let X = y else if z { ... }` — `else if` is meaningless without a
        // value being produced (unlike `if let`). Reject at parse time.
        let source = r#"
function f(r: int | string) -> int {
  let v: int = r else if true { return 0; };
  v
}
"#;
        let (_, errors) = parse_source(source);
        assert!(
            !errors.is_empty(),
            "expected parse error for `let ... else if ...`"
        );
    }

    #[test]
    fn let_else_destructure_pattern_parses() {
        // `let Class { f } = expr else { ... };` — destructure pattern with
        // an else branch; verifies parser handles the structural pattern in
        // combination with the trailing else block.
        let source = r#"
class User { name string }

function f(u: User) -> string {
  let User { name } = u else { return ""; };
  name
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let let_stmt = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::LET_STMT)
            .expect("expected LET_STMT");
        let has_destructure = let_stmt
            .descendants()
            .any(|n| n.kind() == SyntaxKind::DESTRUCTURE_PATTERN);
        assert!(has_destructure, "should contain a destructure pattern");
        let has_else = let_stmt.children_with_tokens().any(
            |elem| matches!(elem, rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::KW_ELSE),
        );
        assert!(has_else, "LET_STMT should have a KW_ELSE token");
    }

    #[test]
    fn let_else_accepted_in_c_style_for_init() {
        // C-style `for (let i = 0; cond; update)` accepts a let-else in the
        // init slot. Makes the loop unreachable (the else diverges), which
        // is silly but not a parse error — same kind of dead code we
        // already allow elsewhere.
        let source = r#"
function f() -> int {
  for (let i: int = 0 else { return 0; }; i < 3; i += 1) {
    let _ = i;
  }
  0
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let for_expr = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::FOR_EXPR)
            .expect("expected FOR_EXPR");
        let let_stmt = for_expr
            .descendants()
            .find(|n| n.kind() == SyntaxKind::LET_STMT)
            .expect("for-init should expose a LET_STMT");
        let has_else = let_stmt.children_with_tokens().any(
            |elem| matches!(elem, rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::KW_ELSE),
        );
        assert!(has_else, "for-init LET_STMT should carry the else branch");
    }

    #[test]
    fn let_without_else_unchanged() {
        // Regression: plain `let x = …;` continues to parse with no else
        // sibling — the new else handling must not turn surrounding tokens
        // into a phantom else block.
        let source = r#"
function f() -> int {
  let x: int = 1;
  x
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let let_stmt = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::LET_STMT)
            .expect("expected LET_STMT");
        let block_count = let_stmt
            .children()
            .filter(|n| n.kind() == SyntaxKind::BLOCK_EXPR)
            .count();
        assert_eq!(block_count, 0, "plain let-stmt should have no BLOCK_EXPR");
    }

    #[test]
    fn postfix_bang_emits_targeted_diagnostic_without_cascade() {
        // Regression: `xs.at(0)!` (TS/Swift/Kotlin non-null assertion) inside
        // an `if` branch used to leave the `!` unconsumed, which the statement
        // loop then re-parsed as a *prefix* `!` on the following `}`/`else`,
        // producing a misleading "expected expression"/"if without else"
        // cascade pointing at unrelated braces. The parser must instead emit a
        // single targeted diagnostic at the `!` and not produce the cascade.
        let source = r#"function f(xs: int[]) -> int {
    if (xs.length() > 0) {
        xs.at(0)!
    } else {
        0
    }
}
"#;
        let (root, errors) = parse_source(source);

        // Exactly one diagnostic, and it points at the `!`.
        assert_eq!(
            errors.len(),
            1,
            "expected exactly one diagnostic at the '!', got: {errors:#?}"
        );
        let msg = format!("{:?}", errors[0]);
        assert!(
            msg.contains("non-null assertion operator"),
            "diagnostic should mention the non-null assertion operator, got: {msg}"
        );

        // The diagnostic span must target the `!` itself (not the preceding
        // operand or a downstream brace), so editors underline the offending
        // character precisely.
        let span = match &errors[0] {
            ParseError::InvalidSyntax { span, .. } => *span,
            other => panic!("expected InvalidSyntax diagnostic, got: {other:?}"),
        };
        let start: usize = span.range.start().into();
        let end: usize = span.range.end().into();
        assert_eq!(
            &source[start..end],
            "!",
            "diagnostic span should cover exactly the '!' token"
        );

        // No cascade: the `else`/`}` confusion is gone.
        assert!(
            !errors.iter().any(|e| {
                let m = format!("{e:?}");
                m.contains("found: \"else\"") || m.contains("found: \"'}'\"")
            }),
            "should not blame `else` or `}}`, got: {errors:#?}"
        );

        // The `if`/`else` structure parsed cleanly: the IF_EXPR still has both
        // its then- and else-block, so downstream layers see a well-formed
        // value-producing `if`.
        let if_expr = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::IF_EXPR)
            .expect("expected IF_EXPR node");
        let block_count = if_expr
            .children()
            .filter(|n| n.kind() == SyntaxKind::BLOCK_EXPR)
            .count();
        assert_eq!(
            block_count, 2,
            "IF_EXPR should keep both then- and else-blocks"
        );
    }

    #[test]
    fn not_equals_is_unaffected_by_postfix_bang_handling() {
        // The postfix `!` handling must not disturb `!=`, which lexes as a
        // single NotEquals token rather than `!` + `=`.
        let source = r#"function f(a: int, b: int) -> bool {
    a != b
}
"#;
        let (_root, errors) = parse_source(source);
        assert_no_errors(&errors);
    }

    #[test]
    fn prefix_bang_on_next_line_is_not_treated_as_postfix() {
        // Regression: BAML separates statements by newlines (no semicolons), so
        // a `!` at the *start of the next line* is a legitimate prefix unary
        // operator on a fresh statement — NOT a stray postfix non-null
        // assertion on the previous statement's value. The trailing-`!` branch
        // must therefore only fire when the `!` directly follows its operand on
        // the same line (guarded by `!has_newline_ahead`).
        let source = r#"function f() -> bool {
    let v: int | string = "hi"
    !(v is int)
}
"#;
        let (_root, errors) = parse_source(source);
        assert_no_errors(&errors);
    }

    #[test]
    fn prefix_bang_starting_chained_boolean_is_not_treated_as_postfix() {
        // The newline-separated prefix `!` must also parse cleanly when it
        // begins a larger boolean expression, e.g. `!(a is int) || (b is int)`.
        let source = r#"function f() -> bool {
    let a: int | string = 1
    let b: int | string = 2
    !(a is int) || (b is int)
}
"#;
        let (_root, errors) = parse_source(source);
        assert_no_errors(&errors);
    }

    #[test]
    fn guard_if_then_parenthesized_return_on_next_line_is_two_statements() {
        // Regression (B-622): a guard `if (cond) { throw ... }` with no else,
        // followed on the next line by a parenthesized return expression, must
        // parse as TWO statements. Without the newline guard on the postfix
        // call branch the parser glued `{ ... }(x * 2)` into a call on the void
        // `if` result, producing a misleading E0006 downstream.
        let source = r#"function f(x: int) -> int {
    if (x < 0) { throw baml.errors.InvalidArgument { message: "neg" } }
    (x * 2)
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        // The `if` and the `(x * 2)` are separate statements: the parenthesized
        // return must NOT be wrapped in a CALL_EXPR on the if-result.
        let call_exprs = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::CALL_EXPR)
            .count();
        assert_eq!(
            call_exprs, 0,
            "the parenthesized return must not be parsed as a call on the void `if` result"
        );
    }

    #[test]
    fn call_with_paren_on_same_line_still_parses_as_call() {
        // The block-close guard must not disturb ordinary calls whose `(`
        // follows the callee on the same line — including multi-line argument
        // lists, where the `(` still sits on the callee's line.
        let source = r#"function f() -> int {
    let a = g(1, 2)
    let b = g(
        3,
        4,
    )
    a + b
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let call_exprs = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::CALL_EXPR)
            .count();
        assert_eq!(
            call_exprs, 2,
            "both same-line and multi-line-argument calls must still parse as calls"
        );
    }

    #[test]
    fn non_block_callee_then_paren_on_next_line_still_chains() {
        // The B-622 fix keys strictly on a block-terminating `}` callee: a
        // *non*-block callee (here a call `g()`, ending in `)`) followed by `(`
        // on the next line must STILL glue into a chained call `g()(1)`. This
        // preserves the deliberately-tested `foo()`-then-`(1)` behavior and
        // guards against a future over-broad "newline separates" change.
        let source = r#"function f(g: () -> (int) -> int) -> int {
    g()
    (1)
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        // `g()(1)` is one CALL_EXPR wrapping another (the outer call applies the
        // returned lambda), so two CALL_EXPR nodes total — the newline did NOT
        // split them into separate statements.
        let call_exprs = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::CALL_EXPR)
            .count();
        assert_eq!(
            call_exprs, 2,
            "a non-block callee followed by `(` on the next line must still chain as a call"
        );
    }

    #[test]
    fn parses_map_property_shorthand() {
        let source = r#"
function build(options: string, retries: int) -> map<string, string | int> {
    { options, retries, explicit: "value" }
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let map = root
            .descendants()
            .find(|node| node.kind() == SyntaxKind::MAP_LITERAL)
            .expect("expected shorthand braces to parse as a map literal");
        let fields = map
            .children()
            .filter(|node| node.kind() == SyntaxKind::OBJECT_FIELD)
            .collect::<Vec<_>>();
        assert_eq!(fields.len(), 3);
        assert!(
            fields[0]
                .children_with_tokens()
                .all(|elem| elem.kind() != SyntaxKind::COLON),
            "the shorthand field must remain distinguishable in the CST"
        );
        assert!(
            fields[2]
                .children_with_tokens()
                .any(|elem| elem.kind() == SyntaxKind::COLON),
            "explicit fields must retain their colon"
        );
    }

    #[test]
    fn parses_object_property_shorthand() {
        let source = r#"
class Request { options string retries int }
function build(options: string, retries: int) -> Request {
    Request { options, retries }
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);

        let object = root
            .descendants()
            .find(|node| node.kind() == SyntaxKind::OBJECT_LITERAL)
            .expect("expected shorthand braces after Request to parse as an object literal");
        let fields = object
            .children()
            .filter(|node| node.kind() == SyntaxKind::OBJECT_FIELD)
            .collect::<Vec<_>>();
        assert_eq!(fields.len(), 2);
        assert!(fields.iter().all(|field| {
            field
                .children_with_tokens()
                .all(|elem| elem.kind() != SyntaxKind::COLON)
        }));
    }

    #[test]
    fn shorthand_lookahead_does_not_consume_control_flow_bodies() {
        let source = r#"
class Flag { enabled bool }

function from_optional(value: int?) -> int {
    if let v: int = value { v } else { 0 }
}

function from_if(flag: bool, value: int) -> int {
    if flag { value } else { 0 }
}

function from_for(values: int[]) -> int {
    for let value in values { value }
    0
}

function parenthesized_object(enabled: bool) -> bool {
    if (Flag { enabled }).enabled { true } else { false }
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        assert_eq!(
            root.descendants()
                .filter(|node| node.kind() == SyntaxKind::OBJECT_LITERAL)
                .count(),
            1,
            "only the explicitly parenthesized constructor should parse as an object literal"
        );
    }

    #[test]
    fn parenthesized_for_iterable_does_not_consume_body_brace() {
        let source = r#"
function iterate(values: int[]) -> int {
    for (let value in values { value }
    0
}
"#;
        let (root, errors) = parse_source(source);
        assert!(
            errors.iter().any(|error| matches!(
                error,
                ParseError::UnexpectedToken {
                    expected,
                    found,
                    ..
                } if expected == "')'" && found == "'{'"
            )),
            "expected the missing ')' diagnostic, got: {errors:#?}"
        );
        assert_eq!(
            root.descendants()
                .filter(|node| node.kind() == SyntaxKind::OBJECT_LITERAL)
                .count(),
            0,
            "the loop body must not be consumed as an object literal"
        );
        assert!(
            root.descendants()
                .filter(|node| node.kind() == SyntaxKind::FOR_EXPR)
                .flat_map(|node| node.children())
                .any(|node| node.kind() == SyntaxKind::BLOCK_EXPR),
            "the brace after the iterable must remain the loop body"
        );
    }

    #[test]
    fn nested_parens_allow_object_literal_in_for_iterable() {
        let source = r#"
class Values { items int[] }

function iterate(items: int[]) -> int {
    for (let item in (Values { items }).items) { item }
    0
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        assert_eq!(
            root.descendants()
                .filter(|node| node.kind() == SyntaxKind::OBJECT_LITERAL)
                .count(),
            1,
            "nested parentheses must opt back into an explicit object literal"
        );
    }

    #[test]
    fn object_literal_is_allowed_in_parenthesized_for_iterable() {
        let source = r#"
class Values { items int[] }

function iterate(items: int[]) -> int {
    for (let item in Values { items }.items) { item }
    0
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        assert_eq!(
            root.descendants()
                .filter(|node| node.kind() == SyntaxKind::OBJECT_LITERAL)
                .count(),
            1,
            "a real header-closing ')' keeps direct object literals unambiguous"
        );
    }

    #[test]
    fn object_literal_is_allowed_in_unparenthesized_for_iterable() {
        let source = r#"
class Values { items int[] }

function iterate(items: int[]) -> int {
    for let item in Values { items }.items { item }
    0
}
"#;
        let (root, errors) = parse_source(source);
        assert_no_errors(&errors);
        assert_eq!(
            root.descendants()
                .filter(|node| node.kind() == SyntaxKind::OBJECT_LITERAL)
                .count(),
            1,
            "an object literal followed by postfix continuation must remain part of the iterable"
        );
        assert!(
            root.descendants()
                .filter(|node| node.kind() == SyntaxKind::FOR_EXPR)
                .flat_map(|node| node.children())
                .any(|node| node.kind() == SyntaxKind::BLOCK_EXPR),
            "the final brace must remain the loop body"
        );
    }
}
