//! The read head over a file's tokens.
//!
//! The [`Input`] is the text, its tokens and the *significant* ones (everything
//! but trivia) in order, built once and never changed. A [`Cursor`] is a
//! position in it, and nothing else: `Copy`, so a lookahead is a copy that
//! walks ahead, and a node's span — from the significant token it started at
//! ([`Mark`]) to the last one consumed — never includes trivia.

use baml_syntax::{
    lexer::lex,
    token::{Ident, KindSet, Token, TokenIdx, TokenKind, TokenSet, TokenType, Tokens},
    tree::TokenRange,
};
use text_size::{TextRange, TextSize};

/// The position a node starts at: the first significant token it will own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Mark(TokenIdx);

impl Mark {
    /// A mark at a token already consumed, for a node that begins with a
    /// child parsed before the node was known (a type's postfix `?`).
    pub(crate) fn at(start: TokenIdx) -> Self {
        Self(start)
    }

    /// A mark at a consumed token: where the node it begins starts.
    pub(crate) fn of<S: TokenSet>(token: Token<S>) -> Self {
        Self(token.idx)
    }

    /// The tokens from this mark to `end` (a [`Cursor::end`], current or
    /// earlier).
    pub(crate) fn to(self, end: TokenIdx) -> TokenRange {
        TokenRange::new(self.0, end.max(self.0))
    }
}

/// What is being parsed: the text, lexed. Immutable once built; every cursor
/// reads it.
pub(crate) struct Input<'t> {
    text: &'t str,
    tokens: Tokens,
    /// The significant (non-trivia) tokens, in order.
    sig: Box<[TokenIdx]>,
}

impl<'t> Input<'t> {
    pub(crate) fn new(text: &'t str) -> Self {
        let tokens = lex(text);
        let sig = tokens
            .iter()
            .filter(|&i| !tokens.kind(i).is_trivia())
            .collect();
        Self { text, tokens, sig }
    }

    /// The tokens, for the tree once the parse is over.
    pub(crate) fn into_tokens(self) -> Tokens {
        self.tokens
    }
}

/// A position in the input: the only state a parse has besides its output.
#[derive(Clone, Copy)]
pub(crate) struct Cursor<'t> {
    input: &'t Input<'t>,
    /// Position in the significant tokens.
    pos: usize,
}

impl<'t> Cursor<'t> {
    pub(crate) fn new(input: &'t Input<'t>) -> Self {
        Self { input, pos: 0 }
    }

    pub(crate) fn tokens(&self) -> &'t Tokens {
        &self.input.tokens
    }

    /// The current position, as an index into the significant tokens.
    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    // ----- what is here -----------------------------------------------------

    /// The kind of the significant token `n` ahead of the current one.
    pub(crate) fn kind(&self, n: usize) -> Option<TokenKind> {
        self.kind_at_sig(self.pos + n)
    }

    /// Significant token `i`.
    pub(crate) fn sig(&self, i: usize) -> Option<TokenIdx> {
        self.input.sig.get(i).copied()
    }

    /// The kind of significant token `i`.
    pub(crate) fn kind_at_sig(&self, i: usize) -> Option<TokenKind> {
        self.input
            .sig
            .get(i)
            .map(|&idx| self.input.tokens.kind(idx))
    }

    /// The position of the first significant token at or after `idx`.
    pub(crate) fn sig_index(&self, idx: TokenIdx) -> usize {
        self.input.sig.partition_point(|&token| token < idx)
    }

    /// The significant-token positions a node's span covers.
    pub(crate) fn sig_range(&self, span: TokenRange) -> std::ops::Range<usize> {
        self.sig_index(span.start())..self.sig_index(span.end())
    }

    pub(crate) fn nth(&self, n: usize) -> Option<TokenIdx> {
        self.input.sig.get(self.pos + n).copied()
    }

    pub(crate) fn text_at(&self, n: usize) -> Option<&'t str> {
        self.nth(n).map(|idx| self.text_of(idx))
    }

    pub(crate) fn text_of(&self, idx: TokenIdx) -> &'t str {
        self.input.tokens.text(self.input.text, idx)
    }

    pub(crate) fn at(&self, kind: TokenKind) -> bool {
        self.kind(0) == Some(kind)
    }

    pub(crate) fn nth_is(&self, n: usize, kind: TokenKind) -> bool {
        self.kind(n) == Some(kind)
    }

    pub(crate) fn at_in<S: TokenSet>(&self) -> bool {
        self.nth_in::<S>(0)
    }

    pub(crate) fn nth_in<S: TokenSet>(&self, n: usize) -> bool {
        self.kind(n).is_some_and(|kind| S::KINDS.contains(kind))
    }

    pub(crate) fn at_set(&self, set: KindSet) -> bool {
        self.kind(0).is_some_and(|kind| set.contains(kind))
    }

    /// A word or contextual keyword: anything that may simply be a name.
    pub(crate) fn at_ident(&self) -> bool {
        self.at_in::<Ident>()
    }

    pub(crate) fn nth_is_ident(&self, n: usize) -> bool {
        self.nth_in::<Ident>(n)
    }

    pub(crate) fn at_end(&self) -> bool {
        self.pos >= self.input.sig.len()
    }

    // ----- consuming ----------------------------------------------------------
    //
    // A token is consumed *typed*, and only when it is what the type says:
    // `eat` returns the `Token<K>` or nothing. There is no untyped consume
    // except `skip`, for tokens a `Bogus` or a raw range will own.

    /// Consume the current significant token if it is a `K`.
    pub(crate) fn eat<K: TokenType>(&mut self) -> Option<Token<K>> {
        self.eat_in::<K>()
    }

    /// Consume the current significant token if its kind is in `S`.
    pub(crate) fn eat_in<S: TokenSet>(&mut self) -> Option<Token<S>> {
        self.eat_set(S::KINDS)
    }

    /// Consume the current significant token if its kind is in `set`, typed
    /// as the `S` the set is declared within (`MEMBER_NAME` within
    /// `NameToken`): the position decides which names it accepts, the node
    /// records a name.
    pub(crate) fn eat_set<S: TokenSet>(&mut self, set: KindSet) -> Option<Token<S>> {
        debug_assert!(set.is_subset(S::KINDS), "a set is consumed as its superset");
        let idx = self.nth(0)?;
        let kind = self.input.tokens.kind(idx);
        set.contains(kind).then(|| {
            self.pos += 1;
            Token::new(idx, kind)
        })
    }

    /// Consume the current significant token without typing it: recovery,
    /// for tokens a `Bogus` or a raw range will own.
    pub(crate) fn skip(&mut self) {
        debug_assert!(!self.at_end(), "`skip` past the end");
        self.pos += 1;
    }

    /// Consume a balanced `{ … }` (a `${` opens a brace too); nothing when not
    /// at `{`. Returns whether the closing brace was found. A token starting a
    /// line while a brace is still open at which `resync` holds (an item head)
    /// means the brace was never closed: the skip stops before it so the item
    /// is parsed, and the caller reports the missing `}`.
    pub(crate) fn skip_balanced_braces(&mut self, resync: impl Fn(Self) -> bool) -> bool {
        if !self.at(TokenKind::LBrace) {
            return true;
        }
        let mut depth = 0u32;
        while let Some(kind) = self.kind(0) {
            match kind {
                TokenKind::LBrace | TokenKind::DollarLBrace => depth += 1,
                TokenKind::RBrace => {
                    depth -= 1;
                    self.skip();
                    if depth == 0 {
                        return true;
                    }
                    continue;
                }
                _ if self.starts_line(0) && resync(*self) => return false,
                _ => {}
            }
            self.skip();
        }
        false
    }

    /// Whether significant token `pos + n` is the first token on its line:
    /// only blanks between it and the previous newline (or the start).
    pub(crate) fn starts_line(&self, n: usize) -> bool {
        let i = self.pos + n;
        let Some(&idx) = self.input.sig.get(i) else {
            return false;
        };
        let gap_start = match i.checked_sub(1) {
            Some(prev) => self.input.sig[prev].0 + 1,
            None => return true,
        };
        (gap_start..idx.0)
            .rev()
            .map(TokenIdx)
            .any(|raw| matches!(self.input.tokens.kind(raw), TokenKind::Newline))
            && (gap_start..idx.0)
                .rev()
                .map(TokenIdx)
                .take_while(|&raw| self.input.tokens.kind(raw) != TokenKind::Newline)
                .all(|raw| self.input.tokens.kind(raw) == TokenKind::Whitespace)
    }

    /// Skip to the next token in `until` at this bracket depth (or an
    /// unbalanced closer, or the end).
    pub(crate) fn skip_until(&mut self, until: KindSet) {
        let mut depth = 0u32;
        while let Some(kind) = self.kind(0) {
            if depth == 0 && until.contains(kind) {
                return;
            }
            match kind {
                TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace | TokenKind::Less => {
                    depth += 1;
                }
                TokenKind::RParen
                | TokenKind::RBracket
                | TokenKind::RBrace
                | TokenKind::Greater => {
                    if depth == 0 {
                        return;
                    }
                    depth -= 1;
                }
                _ => {}
            }
            self.skip();
        }
    }

    // ----- marks and ranges -----------------------------------------------------

    /// The start of the node about to be parsed: the current significant
    /// token, or the end of the text.
    pub(crate) fn mark(&self) -> Mark {
        Mark(self.nth(0).unwrap_or_else(|| self.eof_idx()))
    }

    fn eof_idx(&self) -> TokenIdx {
        TokenIdx(
            u32::try_from(self.input.tokens.len())
                .unwrap_or_else(|_| unreachable!("under 2^32 tokens")),
        )
    }

    /// Just past the last consumed significant token.
    pub(crate) fn end(&self) -> TokenIdx {
        match self.pos.checked_sub(1) {
            Some(last) => TokenIdx(self.input.sig[last].0 + 1),
            None => TokenIdx(0),
        }
    }

    /// The tokens from `mark` to the last one consumed (empty if none).
    pub(crate) fn range_from(&self, mark: Mark) -> TokenRange {
        mark.to(self.end())
    }

    pub(crate) fn consumed_since(&self, mark: Mark) -> bool {
        self.end() > mark.0
    }

    pub(crate) fn text_range(&self, range: TokenRange) -> TextRange {
        if range.is_empty() {
            let at = match range.start().0.checked_sub(1) {
                Some(prev) => self.input.tokens.range(TokenIdx(prev)).end(),
                None => TextSize::from(0),
            };
            return TextRange::empty(at);
        }
        TextRange::new(
            self.input.tokens.range(range.start()).start(),
            self.input.tokens.range(TokenIdx(range.end().0 - 1)).end(),
        )
    }

    pub(crate) fn token_range(&self, idx: TokenIdx) -> TextRange {
        self.input.tokens.range(idx)
    }

    /// Where a "found …" diagnostic points: the current token, or a zero-width
    /// caret just past the last significant token at the end of input (never
    /// at trailing trivia).
    pub(crate) fn error_range(&self) -> TextRange {
        match self.nth(0) {
            Some(idx) => self.input.tokens.range(idx),
            None => TextRange::empty(
                self.input
                    .sig
                    .last()
                    .map_or(TextSize::from(0), |&idx| self.input.tokens.range(idx).end()),
            ),
        }
    }

    // ----- lines ---------------------------------------------------------------------

    /// A line break between the last consumed token and the current one (or
    /// the end of the text): a newline token, or a block comment spanning
    /// lines.
    pub(crate) fn has_newline_ahead(&self) -> bool {
        let to = self.nth(0).unwrap_or_else(|| self.eof_idx());
        (self.end().0..to.0)
            .map(TokenIdx)
            .any(|i| match self.input.tokens.kind(i) {
                TokenKind::Newline => true,
                TokenKind::BlockComment => self.text_of(i).contains('\n'),
                _ => false,
            })
    }

    // ----- lookahead -----------------------------------------------------------------

    /// Whether the body opening at `open` is an LLM body: its first member is
    /// `client:`, `prompt:` or `tools:` (scalar LLM fields require the colon,
    /// so these starts are never expression statements).
    pub(crate) fn looks_like_llm_function_body_from(&self, open: usize) -> bool {
        matches!(
            self.kind_at_sig(open + 1),
            Some(TokenKind::Client | TokenKind::Prompt | TokenKind::Tools)
        ) && self.kind_at_sig(open + 2) == Some(TokenKind::Colon)
    }
}
