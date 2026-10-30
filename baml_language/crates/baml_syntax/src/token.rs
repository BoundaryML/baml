//! Tokens: the lossless substrate every node refers into.

use std::{marker::PhantomData, ops::Range};

use text_size::{TextRange, TextSize};

/// Declares [`TokenKind`], its classification predicates, and one marker type
/// per kind (in `trivia`, `kw`, `ctx`, `punct`, `lit`) for use as `Token<K>`.
macro_rules! define_tokens {
    (
        trivia { $($tr:ident => $trs:literal),* $(,)? }
        keywords { $($kw:ident => $kws:literal),* $(,)? }
        contextual { $($cx:ident => $cxs:literal),* $(,)? }
        punct { $($pu:ident => $pus:literal),* $(,)? }
        literals { $($li:ident => $lis:literal),* $(,)? }
    ) => {
        /// The kind of a token. Every byte of a file belongs to exactly one
        /// token; trivia (whitespace, newlines, comments) are tokens too.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr(u8)]
        pub enum TokenKind {
            $($tr,)* $($kw,)* $($cx,)* $($pu,)* $($li,)*
        }

        impl TokenKind {
            /// Whitespace, newlines and comments: never part of a node's shape.
            pub const fn is_trivia(self) -> bool { matches!(self, $(Self::$tr)|*) }
            /// A reserved word.
            pub const fn is_keyword(self) -> bool { matches!(self, $(Self::$kw)|*) }
            /// A word with meaning only in some positions; an identifier elsewhere.
            pub const fn is_contextual_keyword(self) -> bool { matches!(self, $(Self::$cx)|*) }
            pub const fn is_punct(self) -> bool { matches!(self, $(Self::$pu)|*) }
            pub const fn is_literal(self) -> bool { matches!(self, $(Self::$li)|*) }

            /// The spelling of a fixed token, or a description of a variable one.
            pub const fn describe(self) -> &'static str {
                match self {
                    $(Self::$tr => $trs,)* $(Self::$kw => $kws,)* $(Self::$cx => $cxs,)*
                    $(Self::$pu => $pus,)* $(Self::$li => $lis,)*
                }
            }
        }

        pub mod trivia { $( define_tokens!(@marker $tr); )* }
        pub mod kw { $( define_tokens!(@marker $kw); )* }
        pub mod ctx { $( define_tokens!(@marker $cx); )* }
        pub mod punct { $( define_tokens!(@marker $pu); )* }
        pub mod lit { $( define_tokens!(@marker $li); )* }
    };
    (@marker $name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name;
        impl super::TokenType for $name {
            const KIND: super::TokenKind = super::TokenKind::$name;
        }
    };
}

define_tokens! {
    trivia {
        Whitespace => "whitespace",
        Newline => "newline",
        LineComment => "`//` comment",
        DocComment => "`///` doc comment",
        DirectiveComment => "`//baml:` directive",
        HeaderComment => "`//#` header comment",
        BlockComment => "`/* */` comment",
    }
    keywords {
        Class => "class",
        Enum => "enum",
        Interface => "interface",
        Implements => "implements",
        Implement => "implement",
        Extends => "extends",
        Requires => "requires",
        Function => "function",
        Client => "client",
        Generator => "generator",
        Test => "test",
        Testset => "testset",
        RetryPolicy => "retry_policy",
        TemplateString => "template_string",
        If => "if",
        Else => "else",
        For => "for",
        While => "while",
        Let => "let",
        In => "in",
        Break => "break",
        Continue => "continue",
        Return => "return",
        Throw => "throw",
        Match => "match",
        Catch => "catch",
        CatchAll => "catch_all",
        Throws => "throws",
        Spawn => "spawn",
        Await => "await",
        Defer => "defer",
        Instanceof => "instanceof",
        Is => "is",
    }
    contextual {
        Type => "type",
        Const => "const",
        With => "with",
        As => "as",
        True => "true",
        False => "false",
        Null => "null",
        Endfor => "endfor",
        Endif => "endif",
        Map => "map",
        Env => "env",
        Prompt => "prompt",
        Tools => "tools",
        SelfValue => "self",
        Underscore => "_",
        Spec => "spec",
        Stream => "stream",
        RenderPrompt => "render_prompt",
        BuildRequest => "build_request",
        Parse => "parse",
        CatchAllPanics => "catch_all_panics",
        Unreflect => "unreflect",
        TypeBuilder => "type_builder",
        Dynamic => "dynamic",
    }
    punct {
        LBrace => "{",
        RBrace => "}",
        LParen => "(",
        RParen => ")",
        LBracket => "[",
        RBracket => "]",
        DoubleColon => "::",
        Colon => ":",
        Comma => ",",
        Semicolon => ";",
        DotDotDot => "...",
        DotDot => "..",
        Dot => ".",
        Dollar => "$",
        DollarLBrace => "${",
        Arrow => "->",
        FatArrow => "=>",
        AtAt => "@@",
        At => "@",
        Pipe => "|",
        QuestionDot => "?.",
        Question => "?",
        LessLessEquals => "<<=",
        PlusEquals => "+=",
        MinusEquals => "-=",
        StarEquals => "*=",
        SlashEquals => "/=",
        PercentEquals => "%=",
        AndEquals => "&=",
        PipeEquals => "|=",
        CaretEquals => "^=",
        Equals => "=",
        EqualsEquals => "==",
        NotEquals => "!=",
        LessEquals => "<=",
        GreaterEquals => ">=",
        LessLess => "<<",
        Less => "<",
        Greater => ">",
        AndAnd => "&&",
        OrOr => "||",
        Not => "!",
        And => "&",
        Caret => "^",
        Tilde => "~",
        PlusPlus => "++",
        MinusMinus => "--",
        Plus => "+",
        Minus => "-",
        Star => "*",
        Slash => "/",
        Percent => "%",
        Backslash => "\\",
    }
    literals {
        Word => "identifier",
        Integer => "integer literal",
        Float => "float literal",
        Bigint => "bigint literal",
        String => "string literal",
        RawString => "raw string literal",
        ByteString => "byte string literal",
        LegacyHashString => "`#\"…\"#` string literal",
        BacktickStart => "opening backtick",
        TemplateChunk => "template text",
        BacktickEnd => "closing backtick",
        Header => "`//#` header",
        Error => "unrecognised character",
    }
}

/// A single token kind, usable as `Token<K>`.
pub trait TokenType: Copy + 'static {
    const KIND: TokenKind;
}

/// A set of token kinds a `Token<S>` may have (a single kind, or an
/// alternative such as [`Ident`]).
pub trait TokenSet: Copy + 'static {
    fn contains(kind: TokenKind) -> bool;
}

impl<K: TokenType> TokenSet for K {
    fn contains(kind: TokenKind) -> bool {
        kind == K::KIND
    }
}

/// Anything that names a thing: a word or a contextual keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ident;
impl TokenSet for Ident {
    fn contains(kind: TokenKind) -> bool {
        matches!(kind, TokenKind::Word) || kind.is_contextual_keyword()
    }
}

/// Index of a token in a file's [`Tokens`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TokenIdx(pub u32);

impl TokenIdx {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// A token a node owns, typed by the set of kinds it may have.
pub struct Token<S: TokenSet> {
    pub idx: TokenIdx,
    set: PhantomData<fn() -> S>,
}

impl<S: TokenSet> Token<S> {
    /// Wrap `idx`, whose kind must be in `S`.
    pub fn new(idx: TokenIdx, kind: TokenKind) -> Self {
        debug_assert!(
            S::contains(kind),
            "token {idx:?} of kind {kind:?} is not in this set"
        );
        Self {
            idx,
            set: PhantomData,
        }
    }
}

#[expect(
    clippy::expl_impl_clone_on_copy,
    reason = "deriving would add an `S: Clone` bound the token index does not need"
)]
impl<S: TokenSet> Clone for Token<S> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<S: TokenSet> Copy for Token<S> {}
impl<S: TokenSet> PartialEq for Token<S> {
    fn eq(&self, other: &Self) -> bool {
        self.idx == other.idx
    }
}
impl<S: TokenSet> Eq for Token<S> {}
impl<S: TokenSet> std::hash::Hash for Token<S> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.idx.hash(state);
    }
}
impl<S: TokenSet> std::fmt::Debug for Token<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Token({})", self.idx.0)
    }
}

/// Where an offset falls among the tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenAtOffset {
    /// Past the end of the text, or the file is empty.
    None,
    /// Strictly inside one token.
    Single(TokenIdx),
    /// On the boundary between two tokens: `left` ends and `right` starts here.
    Between { left: TokenIdx, right: TokenIdx },
}

/// A file's tokens as two parallel arrays: kinds, and start offsets with one
/// trailing sentinel (the text length). Tokens are contiguous and cover the
/// whole text, so a token's end is the next token's start — gaps and overlaps
/// are unrepresentable, and `text` is recovered by slicing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Tokens {
    kinds: Vec<TokenKind>,
    starts: Vec<TextSize>,
}

impl Tokens {
    pub fn with_capacity(tokens: usize) -> Self {
        Self {
            kinds: Vec::with_capacity(tokens),
            starts: Vec::with_capacity(tokens + 1),
        }
    }

    /// Append a token starting at `start`; starts must be non-decreasing.
    pub fn push(&mut self, kind: TokenKind, start: TextSize) {
        debug_assert!(
            self.starts.last().is_none_or(|&last| last <= start),
            "token starts must be non-decreasing"
        );
        debug_assert_eq!(self.kinds.len(), self.starts.len(), "push after finish");
        self.kinds.push(kind);
        self.starts.push(start);
    }

    /// Close the array with the text length. After this, no more pushes.
    pub fn finish(&mut self, text_len: TextSize) {
        debug_assert_eq!(self.kinds.len(), self.starts.len(), "finish called twice");
        debug_assert!(self.starts.last().is_none_or(|&last| last <= text_len));
        self.starts.push(text_len);
    }

    pub fn len(&self) -> usize {
        self.kinds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    pub fn kind(&self, idx: TokenIdx) -> TokenKind {
        self.kinds[idx.index()]
    }

    pub fn range(&self, idx: TokenIdx) -> TextRange {
        debug_assert_eq!(
            self.starts.len(),
            self.kinds.len() + 1,
            "`finish` has not been called"
        );
        TextRange::new(self.starts[idx.index()], self.starts[idx.index() + 1])
    }

    pub fn text<'s>(&self, source: &'s str, idx: TokenIdx) -> &'s str {
        &source[self.range(idx)]
    }

    pub fn iter(&self) -> impl Iterator<Item = TokenIdx> + '_ {
        (0..self.kinds.len()).map(|i| {
            TokenIdx(
                u32::try_from(i).unwrap_or_else(|_| unreachable!("a file never has 2^32 tokens")),
            )
        })
    }

    /// Tokens in `range`, in order.
    pub fn between(&self, range: Range<TokenIdx>) -> impl Iterator<Item = TokenIdx> + '_ {
        (range.start.0..range.end.0).map(TokenIdx)
    }

    /// Whether `idx` follows the previous token with no trivia between them —
    /// how `??`, `>>` and `>>=` are read from single-character tokens. (Tokens
    /// cover the text contiguously, so "adjacent" can only mean "nothing but
    /// another significant token before it".)
    pub fn joined_to_prev(&self, idx: TokenIdx) -> bool {
        idx.0 > 0 && !self.kind(TokenIdx(idx.0 - 1)).is_trivia()
    }

    pub fn prev_significant(&self, idx: TokenIdx) -> Option<TokenIdx> {
        (0..idx.0)
            .rev()
            .map(TokenIdx)
            .find(|&i| !self.kind(i).is_trivia())
    }

    pub fn next_significant(&self, idx: TokenIdx) -> Option<TokenIdx> {
        (idx.0 + 1
            ..u32::try_from(self.len())
                .unwrap_or_else(|_| unreachable!("a file never has 2^32 tokens")))
            .map(TokenIdx)
            .find(|&i| !self.kind(i).is_trivia())
    }

    /// The token containing `offset`, or the pair it sits between.
    pub fn token_at_offset(&self, offset: TextSize) -> TokenAtOffset {
        if self.is_empty() || offset > self.starts[self.len()] {
            return TokenAtOffset::None;
        }
        // The last token whose start is <= offset.
        let idx = match self.starts[..self.len()].binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let idx_u32 =
            u32::try_from(idx).unwrap_or_else(|_| unreachable!("bounded by the token count"));
        if self.starts[idx] == offset {
            if idx == 0 {
                return TokenAtOffset::Single(TokenIdx(0));
            }
            return TokenAtOffset::Between {
                left: TokenIdx(idx_u32 - 1),
                right: TokenIdx(idx_u32),
            };
        }
        if offset == self.starts[self.len()] {
            return TokenAtOffset::None;
        }
        TokenAtOffset::Single(TokenIdx(idx_u32))
    }
}
