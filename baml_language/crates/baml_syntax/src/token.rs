//! Tokens: the lossless substrate every node refers into.
//!
//! [`TokenKind`] is written once, below, as a Logos token enum. Its
//! `#[token]`/`#[regex]` attributes drive the lexer ([`crate::lexer`]), and
//! `#[derive(TokenTable)]` reads the same attributes to produce everything
//! else: `TokenKind::ALL`, the group predicates, `describe()`, the `Token<K>`
//! marker types in [`trivia`]/[`kw`]/[`ctx`]/[`punct`]/[`lit`], and the
//! [`Token!`](crate::Token)/[`TokenKind!`](crate::TokenKind) macros.

use std::{marker::PhantomData, ops::Range};

use logos::Logos;
use text_size::{TextRange, TextSize};

use crate::{
    TokenTable,
    lexer::{block_comment, hash, line_comment, quoted_body},
};

/// The kind of a token. Every byte of a file belongs to exactly one token;
/// trivia (whitespace, newlines, comments) are tokens too.
///
/// The lexer reports nothing: what it could not lex is in the kinds. Text no
/// pattern matched is [`Error`](Self::Error); a delimited literal with no
/// closer is its `Unclosed*` kind (the body still runs as far as the literal
/// would); an unclosed template simply has no `BacktickEnd`. The parser turns
/// those into diagnostics, so there is one source of diagnostics.
///
/// Kinds without a pattern are produced by a callback or the lexer's driver
/// rather than a match: the `//` comment kinds (one rule classifies them), the
/// template-text kinds, [`Error`](Self::Error) and the `Unclosed*` kinds.
/// `Default` is `Error`, which Logos requires of its error value: a failed
/// match yields it, so every lexer step yields a kind.
#[derive(Logos, TokenTable, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[logos(error = TokenKind)]
#[repr(u8)]
pub enum TokenKind {
    // ----- trivia -----------------------------------------------------------
    #[group(trivia)]
    #[describe = "whitespace"]
    #[regex(r"[ \t]+")]
    Whitespace,
    #[group(trivia)]
    #[describe = "newline"]
    #[regex(r"\r?\n")]
    Newline,
    /// `//…` to the end of the line, or a `#!…` shebang line (a line comment
    /// anywhere, not only on the first line). The `//` rule classifies the
    /// three comment kinds below by what follows the slashes; `#!` is the
    /// `#` rule's doing.
    #[group(trivia)]
    #[describe = "`//` comment"]
    #[token("//", line_comment)]
    LineComment,
    /// `///…`.
    #[group(trivia)]
    #[describe = "`///` doc comment"]
    DocComment,
    /// `//baml:…`.
    #[group(trivia)]
    #[describe = "`//baml:` directive"]
    DirectiveComment,
    /// `//#…`.
    #[group(trivia)]
    #[describe = "`//#` header comment"]
    HeaderComment,
    /// Not nested; closes at the first `*/`.
    #[group(trivia)]
    #[describe = "`/* */` comment"]
    #[token("/*", block_comment)]
    BlockComment,
    /// A `/*` with no `*/` before the end of the text: trivia, so the parse
    /// goes on, and reported by the parser.
    #[group(trivia)]
    #[describe = "unclosed `/*` comment"]
    UnclosedBlockComment,

    // ----- keywords ---------------------------------------------------------
    #[group(keyword)]
    #[token("class")]
    Class,
    #[group(keyword)]
    #[token("enum")]
    Enum,
    #[group(keyword)]
    #[token("interface")]
    Interface,
    #[group(keyword)]
    #[token("implements")]
    Implements,
    #[group(keyword)]
    #[token("implement")]
    Implement,
    #[group(keyword)]
    #[token("extends")]
    Extends,
    #[group(keyword)]
    #[token("requires")]
    Requires,
    #[group(keyword)]
    #[token("function")]
    Function,
    #[group(keyword)]
    #[token("client")]
    Client,
    #[group(keyword)]
    #[token("generator")]
    Generator,
    #[group(keyword)]
    #[token("test")]
    Test,
    #[group(keyword)]
    #[token("testset")]
    Testset,
    #[group(keyword)]
    #[token("retry_policy")]
    RetryPolicy,
    #[group(keyword)]
    #[token("template_string")]
    TemplateString,
    #[group(keyword)]
    #[token("if")]
    If,
    #[group(keyword)]
    #[token("else")]
    Else,
    #[group(keyword)]
    #[token("for")]
    For,
    #[group(keyword)]
    #[token("while")]
    While,
    #[group(keyword)]
    #[token("let")]
    Let,
    #[group(keyword)]
    #[token("in")]
    In,
    #[group(keyword)]
    #[token("break")]
    Break,
    #[group(keyword)]
    #[token("continue")]
    Continue,
    #[group(keyword)]
    #[token("return")]
    Return,
    #[group(keyword)]
    #[token("throw")]
    Throw,
    #[group(keyword)]
    #[token("match")]
    Match,
    #[group(keyword)]
    #[token("catch")]
    Catch,
    #[group(keyword)]
    #[token("catch_all")]
    CatchAll,
    #[group(keyword)]
    #[token("throws")]
    Throws,
    #[group(keyword)]
    #[token("spawn")]
    Spawn,
    #[group(keyword)]
    #[token("await")]
    Await,
    #[group(keyword)]
    #[token("defer")]
    Defer,
    #[group(keyword)]
    #[token("instanceof")]
    Instanceof,
    #[group(keyword)]
    #[token("is")]
    Is,

    // ----- contextual keywords: a name anywhere the grammar does not give
    // ----- them meaning. Logos prefers a `#[token]` over a regex of the same
    // ----- length, so these win over `Word`; a longer word (`type-x`) is a `Word`.
    #[group(contextual)]
    #[token("type")]
    Type,
    #[group(contextual)]
    #[token("const")]
    Const,
    #[group(contextual)]
    #[token("with")]
    With,
    #[group(contextual)]
    #[token("as")]
    As,
    #[group(contextual)]
    #[token("true")]
    True,
    #[group(contextual)]
    #[token("false")]
    False,
    #[group(contextual)]
    #[token("null")]
    Null,
    #[group(contextual)]
    #[token("endfor")]
    Endfor,
    #[group(contextual)]
    #[token("endif")]
    Endif,
    #[group(contextual)]
    #[token("map")]
    Map,
    #[group(contextual)]
    #[token("env")]
    Env,
    #[group(contextual)]
    #[token("prompt")]
    Prompt,
    #[group(contextual)]
    #[token("tools")]
    Tools,
    #[group(contextual)]
    #[token("self")]
    SelfValue,
    /// Spelled by a one-character word, so it needs an explicit edge over
    /// the `Word` regex (longer keywords win on length alone).
    #[group(contextual)]
    #[token("_", priority = 3)]
    Underscore,
    #[group(contextual)]
    #[token("spec")]
    Spec,
    #[group(contextual)]
    #[token("stream")]
    Stream,
    #[group(contextual)]
    #[token("render_prompt")]
    RenderPrompt,
    #[group(contextual)]
    #[token("build_request")]
    BuildRequest,
    #[group(contextual)]
    #[token("parse")]
    Parse,
    #[group(contextual)]
    #[token("catch_all_panics")]
    CatchAllPanics,
    #[group(contextual)]
    #[token("unreflect")]
    Unreflect,
    #[group(contextual)]
    #[token("type_builder")]
    TypeBuilder,
    #[group(contextual)]
    #[token("dynamic")]
    Dynamic,

    // ----- punctuation. `>>`, `>>=` and `??` are deliberately absent (see
    // ----- `crate::ops`); `${` is only produced inside template text.
    #[group(punct)]
    #[token("{")]
    LBrace,
    #[group(punct)]
    #[token("}")]
    RBrace,
    #[group(punct)]
    #[token("(")]
    LParen,
    #[group(punct)]
    #[token(")")]
    RParen,
    #[group(punct)]
    #[token("[")]
    LBracket,
    #[group(punct)]
    #[token("]")]
    RBracket,
    #[group(punct)]
    #[token("::")]
    DoubleColon,
    #[group(punct)]
    #[token(":")]
    Colon,
    #[group(punct)]
    #[token(",")]
    Comma,
    #[group(punct)]
    #[token(";")]
    Semicolon,
    #[group(punct)]
    #[token("...")]
    DotDotDot,
    #[group(punct)]
    #[token("..")]
    DotDot,
    #[group(punct)]
    #[token(".")]
    Dot,
    #[group(punct)]
    #[token("$")]
    Dollar,
    /// Produced by the driver inside template text, where `$` touches `{`;
    /// in code `${` is `$` `{`.
    #[group(punct)]
    #[spelling = "${"]
    DollarLBrace,
    #[group(punct)]
    #[token("->")]
    Arrow,
    #[group(punct)]
    #[token("=>")]
    FatArrow,
    #[group(punct)]
    #[token("@@")]
    AtAt,
    #[group(punct)]
    #[token("@")]
    At,
    #[group(punct)]
    #[token("|")]
    Pipe,
    #[group(punct)]
    #[token("?.")]
    QuestionDot,
    #[group(punct)]
    #[token("?")]
    Question,
    #[group(punct)]
    #[token("<<=")]
    LessLessEquals,
    #[group(punct)]
    #[token("+=")]
    PlusEquals,
    #[group(punct)]
    #[token("-=")]
    MinusEquals,
    #[group(punct)]
    #[token("*=")]
    StarEquals,
    #[group(punct)]
    #[token("/=")]
    SlashEquals,
    #[group(punct)]
    #[token("%=")]
    PercentEquals,
    #[group(punct)]
    #[token("&=")]
    AndEquals,
    #[group(punct)]
    #[token("|=")]
    PipeEquals,
    #[group(punct)]
    #[token("^=")]
    CaretEquals,
    #[group(punct)]
    #[token("=")]
    Equals,
    #[group(punct)]
    #[token("==")]
    EqualsEquals,
    #[group(punct)]
    #[token("!=")]
    NotEquals,
    #[group(punct)]
    #[token("<=")]
    LessEquals,
    #[group(punct)]
    #[group(punct)]
    #[token("<<")]
    LessLess,
    #[group(punct)]
    #[token("<")]
    Less,
    #[group(punct)]
    #[token(">")]
    Greater,
    #[group(punct)]
    #[token("&&")]
    AndAnd,
    #[group(punct)]
    #[token("||")]
    OrOr,
    #[group(punct)]
    #[token("!")]
    Not,
    #[group(punct)]
    #[token("&")]
    And,
    #[group(punct)]
    #[token("^")]
    Caret,
    #[group(punct)]
    #[token("~")]
    Tilde,
    #[group(punct)]
    #[token("++")]
    PlusPlus,
    #[group(punct)]
    #[token("--")]
    MinusMinus,
    #[group(punct)]
    #[token("+")]
    Plus,
    #[group(punct)]
    #[token("-")]
    Minus,
    #[group(punct)]
    #[token("*")]
    Star,
    #[group(punct)]
    #[token("/")]
    Slash,
    #[group(punct)]
    #[token("%")]
    Percent,
    #[group(punct)]
    #[token("\\")]
    Backslash,

    // ----- literals and other variable-text kinds ---------------------------
    /// `name`, `gpt-4o` (hyphens after the first character), `$name`,
    /// `Foo$bar`; never ends in `$`.
    #[group(literal)]
    #[describe = "identifier"]
    #[regex(r"\$[a-zA-Z_][a-zA-Z0-9_]*")]
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_-]*(\$[a-zA-Z_][a-zA-Z0-9_-]*)*")]
    Word,
    /// Decimal, hex, octal or binary. The prefixed forms over-accept so a
    /// malformed literal stays one token; validation reports it precisely.
    #[group(literal)]
    #[describe = "integer literal"]
    #[regex(r"[0-9][0-9_]*")]
    #[regex(r"0[xX][0-9a-fA-F_]*")]
    #[regex(r"0[oO][0-9_]*")]
    #[regex(r"0[bB][0-9_]*")]
    Integer,
    /// A decimal fraction and/or an exponent whose digits start right after
    /// the sign (`1e_1` is not a float, unlike Rust).
    #[group(literal)]
    #[describe = "float literal"]
    #[regex(r"[0-9][0-9_]*\.[0-9][0-9_]*")]
    #[regex(r"[0-9][0-9_]*(\.[0-9][0-9_]*)?[eE][+-]?[0-9][0-9_]*")]
    Float,
    /// An integer form with an `n` suffix.
    #[group(literal)]
    #[describe = "bigint literal"]
    #[regex(r"[0-9][0-9_]*n")]
    #[regex(r"0[xX][0-9a-fA-F_]*n")]
    #[regex(r"0[oO][0-9_]*n")]
    #[regex(r"0[bB][0-9_]*n")]
    Bigint,
    /// `"…"`; `\` escapes the next character.
    #[group(literal)]
    #[describe = "string literal"]
    #[token("\"", quoted_body)]
    String,
    /// `r"…"` — the prefix must touch the quote and be exactly `r`.
    #[group(literal)]
    #[describe = "raw string literal"]
    #[token("r\"", quoted_body)]
    RawString,
    /// `b"…"`.
    #[group(literal)]
    #[describe = "byte string literal"]
    #[token("b\"", quoted_body)]
    ByteString,
    /// `#"…"#`, closed by a `"` followed by exactly as many `#` as opened
    /// it; blanks and newlines may separate the hashes. Removed syntax, kept
    /// so it is diagnosed as such. Produced by the `#` rule.
    #[group(literal)]
    #[describe = "`#\"…\"#` string literal"]
    LegacyHashString,
    /// A run of one or more backticks that opens a template (BEP-049 §8: an
    /// N-tick opener is greedy; with no close ahead, the rest of the text is
    /// its content and the parser reports the missing close).
    #[group(literal)]
    #[describe = "opening backtick"]
    #[regex("`+")]
    BacktickStart,
    #[group(literal)]
    #[describe = "template text"]
    TemplateChunk,
    #[group(literal)]
    #[describe = "closing backtick"]
    BacktickEnd,

    // ----- invalid: lexed, but not as anything the grammar has -----------------
    /// A `#` that opens neither a `#"…"#` string nor a `#!` line: one token,
    /// so the blanks after it stay trivia and a newline is still seen. The
    /// `#` rule's callback tells the three apart; a regex for the string
    /// opener left Logos reporting a failed `#…"` as this kind with the
    /// failed bytes in its span.
    #[group(invalid)]
    #[describe = "`#`"]
    #[token("#", hash)]
    Hash,
    /// A `"…` with no closing quote before the end of the text.
    #[group(invalid)]
    #[describe = "unclosed string literal"]
    UnclosedString,
    #[group(invalid)]
    #[describe = "unclosed raw string literal"]
    UnclosedRawString,
    #[group(invalid)]
    #[describe = "unclosed byte string literal"]
    UnclosedByteString,
    #[group(invalid)]
    #[describe = "unclosed `#\"…\"#` string literal"]
    UnclosedHashString,
    /// Text no pattern matched.
    #[default]
    #[group(invalid)]
    #[describe = "unrecognised character"]
    Error,
}

/// A single token kind, usable as `Token<K>`.
pub trait TokenType: Copy + 'static {
    const KIND: TokenKind;
}

/// A set of token kinds as a bitset: `const`-constructible, so grammar tables
/// (what starts an item, where recovery stops) are data, and membership is
/// one shift and mask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KindSet {
    bits: [u64; Self::WORDS],
}

const _: () = assert!(TokenKind::ALL.len() <= KindSet::WORDS * 64);
// A kind's bit is its declaration index.
const _: () = {
    let mut i = 0;
    while i < TokenKind::ALL.len() {
        assert!(
            TokenKind::ALL[i] as usize == i,
            "`TokenKind::ALL` is in declaration order"
        );
        i += 1;
    }
};

impl KindSet {
    const WORDS: usize = 3;
    pub const EMPTY: Self = Self {
        bits: [0; Self::WORDS],
    };

    pub const fn new(kinds: &[TokenKind]) -> Self {
        let mut set = Self::EMPTY;
        let mut i = 0;
        while i < kinds.len() {
            set = set.insert(kinds[i]);
            i += 1;
        }
        set
    }

    pub const fn single(kind: TokenKind) -> Self {
        Self::EMPTY.insert(kind)
    }

    #[must_use]
    pub const fn insert(mut self, kind: TokenKind) -> Self {
        let bit = kind as usize;
        self.bits[bit / 64] |= 1 << (bit % 64);
        self
    }

    #[must_use]
    pub const fn union(mut self, other: Self) -> Self {
        let mut i = 0;
        while i < Self::WORDS {
            self.bits[i] |= other.bits[i];
            i += 1;
        }
        self
    }

    pub const fn contains(self, kind: TokenKind) -> bool {
        let bit = kind as usize;
        self.bits[bit / 64] & (1 << (bit % 64)) != 0
    }

    /// Whether every kind in `self` is in `other`.
    pub const fn is_subset(self, other: Self) -> bool {
        let mut i = 0;
        while i < Self::WORDS {
            if self.bits[i] & !other.bits[i] != 0 {
                return false;
            }
            i += 1;
        }
        true
    }

    /// Every kind for which `TokenKind::is_contextual_keyword` holds.
    pub const CONTEXTUAL_KEYWORDS: Self = {
        let mut set = Self::EMPTY;
        let mut i = 0;
        while i < TokenKind::ALL.len() {
            if TokenKind::ALL[i].is_contextual_keyword() {
                set = set.insert(TokenKind::ALL[i]);
            }
            i += 1;
        }
        set
    };
}

/// A set of token kinds a `Token<S>` may have (a single kind, or an
/// alternative such as [`Ident`]). [`KINDS`](Self::KINDS) is the one
/// definition; `contains` reads it.
pub trait TokenSet: Copy + 'static {
    const KINDS: KindSet;
}

impl<K: TokenType> TokenSet for K {
    const KINDS: KindSet = KindSet::single(K::KIND);
}

/// Anything that names a thing: a word or a contextual keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ident;
impl TokenSet for Ident {
    const KINDS: KindSet = KindSet::CONTEXTUAL_KEYWORDS.insert(TokenKind::Word);
}

/// Declares token sets: alternatives a `Token<S>` field may hold.
macro_rules! token_set {
    ($($(#[$meta:meta])* $name:ident = $($kind:ident)|+;)*) => {
        $(
            $(#[$meta])*
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
            pub struct $name;
            impl super::TokenSet for $name {
                const KINDS: super::KindSet = super::KindSet::new(&[$(super::TokenKind::$kind),+]);
            }
        )*
    };
}

/// Token alternatives used by node fields.
pub mod sets {
    use super::{Ident, KindSet, TokenKind, TokenSet};

    token_set! {
        /// `,` or `;` after a member.
        MemberSep = Comma | Semicolon;
        /// `let` or `const`.
        LetIntroducer = Let | Const;
        /// `->`, or `=>` accepted in its place.
        AnyArrow = Arrow | FatArrow;
        ImplementsKw = Implements | Implement;
        TestKw = Test | Testset;
        CatchKw = Catch | CatchAll | CatchAllPanics;
        /// The keyword of a removed item form, kept only for its diagnostic.
        RemovedKw = Client | RetryPolicy | TemplateString | Generator | TypeBuilder | Dynamic | Test;
        LiteralExprToken = Integer | Float | Bigint | True | False | Null;
        /// A string literal, closed or not: the parser accepts an unclosed one
        /// as the literal it was meant to be and reports the missing closer.
        StringToken = String | RawString | ByteString | LegacyHashString
            | UnclosedString | UnclosedRawString | UnclosedByteString | UnclosedHashString;
        LiteralTypeToken = Integer | Float | Bigint | String | RawString | ByteString | LegacyHashString
            | UnclosedString | UnclosedRawString | UnclosedByteString | UnclosedHashString;
        /// `.` or the `$` member separator.
        MemberDot = Dot | Dollar;
        /// The selector of `F@spec`, `F@stream`, ….
        SpecSelector = Spec | Stream | RenderPrompt | BuildRequest | Parse;
        EndTagKw = Endfor | Endif;
    }

    /// Any token that may name something: a word, a contextual keyword, or a
    /// reserved word the grammar accepts as a name in some position (`client`
    /// as a field, `class` as a path segment, …). Which positions accept which
    /// is the parser's decision; the tree records the token it accepted.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct NameToken;
    impl TokenSet for NameToken {
        const KINDS: KindSet = Ident::KINDS.union(KindSet::new(&[
            TokenKind::Client,
            TokenKind::Class,
            TokenKind::Enum,
            TokenKind::Function,
            TokenKind::Interface,
            TokenKind::Implements,
            TokenKind::Implement,
            TokenKind::Extends,
            TokenKind::Requires,
            TokenKind::Spawn,
            TokenKind::Await,
            TokenKind::Throws,
        ]));
    }
}

/// The names `Token![Name]` resolves: kinds whose text varies (`Word`,
/// `String`, `TemplateChunk`, …) and token sets (`Ident`, `LetIntroducer`, …).
pub mod by_name {
    // A trivia token is a node's field in exactly one place: a `//#` line that
    // is a statement.
    pub use super::{Ident, lit::*, sets::*, trivia::HeaderComment};
    pub use crate::ops::UnaryOpToken;
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
            S::KINDS.contains(kind),
            "token {idx:?} of kind {kind:?} is not in this set"
        );
        Self {
            idx,
            set: PhantomData,
        }
    }

    /// The same token as a member of the superset `T`; `S ⊆ T` is checked
    /// at compile time.
    pub fn widen<T: TokenSet>(self) -> Token<T> {
        const {
            assert!(
                S::KINDS.is_subset(T::KINDS),
                "a token is widened only to a superset of its kinds"
            );
        }
        Token {
            idx: self.idx,
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
///
/// Only [`crate::lexer::lex`] constructs one, in a single step, so there is no
/// half-built state to observe.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Tokens {
    kinds: Box<[TokenKind]>,
    starts: Box<[TextSize]>,
}

impl Tokens {
    /// `starts` holds one start per kind plus the text length as sentinel,
    /// from zero and strictly increasing (no token is empty).
    pub(crate) fn from_parts(kinds: Vec<TokenKind>, starts: Vec<TextSize>) -> Self {
        debug_assert_eq!(
            starts.len(),
            kinds.len() + 1,
            "one start per token plus the sentinel"
        );
        debug_assert_eq!(
            starts[0],
            TextSize::from(0),
            "the first token starts the text"
        );
        debug_assert!(
            starts.windows(2).all(|w| w[0] < w[1]),
            "token starts must be strictly increasing"
        );
        Self {
            kinds: kinds.into_boxed_slice(),
            starts: starts.into_boxed_slice(),
        }
    }

    /// The kind of `idx`, if it is a token of this file.
    pub fn get(&self, idx: TokenIdx) -> Option<TokenKind> {
        self.kinds.get(idx.index()).copied()
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

    /// Whether `idx` follows the previous token with no trivia between them.
    /// (Tokens cover the text contiguously, so "adjacent" can only mean
    /// "nothing but another significant token before it".)
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
        if self.is_empty() || offset >= self.starts[self.len()] {
            return TokenAtOffset::None;
        }
        // The last token whose start is <= offset.
        let idx = match self.starts[..self.len()].binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let idx_u32 =
            u32::try_from(idx).unwrap_or_else(|_| unreachable!("bounded by the token count"));
        if self.starts[idx] == offset && idx > 0 {
            return TokenAtOffset::Between {
                left: TokenIdx(idx_u32 - 1),
                right: TokenIdx(idx_u32),
            };
        }
        TokenAtOffset::Single(TokenIdx(idx_u32))
    }
}
