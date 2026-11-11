//! The parser: a [`Cursor`] over the tokens, a [`Sink`] for the tree, and the
//! combinators that need both. Grammar code lives in the grammar modules as
//! free functions (or [`Parse`] impls) over `p`; the only state is the
//! cursor's position and the sink's output. Nothing about *where in the
//! grammar* a parse is travels with it: what depends on a construct's
//! surroundings is decided from the finished tree (E0175), and a restriction
//! on how tokens are read arrives as a parameter of the functions that apply
//! it.

use baml_syntax::{
    Token,
    nodes::{Arenas, Name},
    token::{KindSet, Token, TokenIdx, TokenKind, TokenSet, TokenType, sets::NameToken},
    tree::{
        AnyNode, AnyNodeId, Bogus, HasArena, HasPool, Id, Missing, NodeMarker, PoolItem, Sep, Sep1,
        SepItem, TokenRange,
    },
};
use text_size::TextRange;

use crate::{
    cursor::{Cursor, Mark},
    error::ParseError,
    sink::Sink,
};

/// The position in the input and the output of one parse. A speculative
/// parse gets a child with a copy of the cursor and a
/// [`Speculation`](baml_syntax::nodes::Speculation) over the same arenas; the
/// parent is unreachable until the child is done, and takes the child's
/// position back afterwards.
pub(crate) struct Parser<'t, 'a> {
    pub cur: Cursor<'t>,
    pub out: Sink<'a>,
    /// How many expressions, types, patterns and blocks are open: the
    /// grammar recurses once per level, and [`MAX_NESTING`] bounds it.
    pub depth: u32,
}

/// The deepest nesting of expressions, types, patterns and blocks the parser
/// reads; past it the construct is reported and skipped. Real code is nowhere
/// near; the bound keeps a pathological file from exhausting the stack of the
/// thread the compiler runs on (2 MiB on a worker), and lowering recurses
/// over the same shape.
pub(crate) const MAX_NESTING: u32 = 128;

/// A node that parses itself after its head. The head is what the caller
/// consumed to know that one of these begins here — its keyword, its opening
/// bracket — and is the node's first fields, so the node cannot be parsed
/// where it does not begin and nothing is read twice. What follows the head
/// may be incomplete; the node records that with `Missing`/`Bogus`.
pub(crate) trait Parse: NodeMarker + Sized
where
    Arenas: HasArena<Self>,
{
    /// The tokens that identified the node, consumed by the caller.
    type Head;
    fn parse(p: &mut Parser<'_, '_>, head: Self::Head) -> Id<Self>;
}

impl<'t, 'a> Parser<'t, 'a> {
    pub(crate) fn new(cur: Cursor<'t>, out: Sink<'a>) -> Self {
        Self { cur, out, depth: 0 }
    }

    /// Allocate `node` spanning `mark` to the last consumed token.
    pub(crate) fn alloc<T: NodeMarker>(&mut self, mark: Mark, node: T) -> Id<T>
    where
        Arenas: HasArena<T>,
    {
        let span = self.cur.range_from(mark);
        self.out.alloc(node, span)
    }

    /// Allocate `node` spanning `mark` to `end`, for a wrapper decided after
    /// its children were parsed (`end` is the [`Cursor::end`] recorded then).
    pub(crate) fn alloc_to<T: NodeMarker>(&mut self, mark: Mark, end: TokenIdx, node: T) -> Id<T>
    where
        Arenas: HasArena<T>,
    {
        let span = mark.to(end);
        self.out.alloc(node, span)
    }

    /// A `Name` from the current token if its kind is in `set` (a subset of
    /// the name tokens): the position decides which names it accepts.
    pub(crate) fn eat_name(&mut self, set: KindSet) -> Option<Id<Name>> {
        let token = self.cur.eat_set::<NameToken>(set)?;
        Some(self.name(token))
    }

    /// The `Name` node of a consumed name token: exactly that token, however
    /// much has been read since.
    pub(crate) fn name(&mut self, token: Token![NameToken]) -> Id<Name> {
        self.alloc_to(Mark::of(token), TokenIdx(token.idx.0 + 1), Name { token })
    }

    /// A `Name` from `set`, or a reported [`Missing`] named `expected`.
    pub(crate) fn expect_name(
        &mut self,
        set: KindSet,
        expected: &str,
    ) -> Result<Id<Name>, Missing> {
        self.eat_name(set).ok_or_else(|| self.missing(expected))
    }

    /// Where the node `id` starts: the mark for a wrapper built around it
    /// after it was parsed (a binary expression around its left operand).
    pub(crate) fn mark_of(&self, id: AnyNodeId) -> Mark {
        Mark::at(self.out.span(id).start())
    }

    /// [`Parser::mark_of`] for a union whose every alternative is a node.
    pub(crate) fn mark_of_any(&self, node: &impl AnyNode) -> Mark {
        let id = node
            .any_node_id()
            .unwrap_or_else(|| unreachable!("every alternative of the union is a node"));
        self.mark_of(id)
    }

    // ----- diagnostics at the cursor ------------------------------------------------

    /// "Expected `expected`, found <the current token>".
    pub(crate) fn unexpected_error(&self, expected: impl Into<String>) -> ParseError {
        let range = self.cur.error_range();
        match self.cur.kind(0) {
            Some(kind) => ParseError::UnexpectedToken {
                expected: expected.into(),
                found: found_name(kind),
                range,
            },
            None => ParseError::UnexpectedEof {
                expected: expected.into(),
                range,
            },
        }
    }

    /// Report "expected …, found …" at the current token and continue.
    pub(crate) fn unexpected(&mut self, expected: impl Into<String>) {
        let error = self.unexpected_error(expected);
        self.out.report(error);
    }

    pub(crate) fn error(&mut self, message: impl Into<String>, range: TextRange) {
        self.out.report(ParseError::InvalidSyntax {
            message: message.into(),
            range,
        });
    }

    pub(crate) fn error_here(&mut self, message: impl Into<String>) {
        let range = self.cur.error_range();
        self.error(message, range);
    }

    pub(crate) fn removed(&mut self, message: impl Into<String>, range: TextRange) {
        self.out.report(ParseError::RemovedFeature {
            message: message.into(),
            range,
        });
    }

    // ----- missing ---------------------------------------------------------------------

    /// The token, or a reported [`Missing`] named by the token's own spelling.
    pub(crate) fn expect<K: TokenType>(&mut self) -> Result<Token<K>, Missing> {
        self.cur
            .eat::<K>()
            .ok_or_else(|| self.missing(found_name(K::KIND)))
    }

    /// One of a set of tokens, or a reported [`Missing`] named by `expected`.
    pub(crate) fn expect_in<S: TokenSet>(&mut self, expected: &str) -> Result<Token<S>, Missing> {
        self.cur.eat_in::<S>().ok_or_else(|| self.missing(expected))
    }

    /// The token, or a reported [`Missing`] named by `expected` rather than
    /// by the token's spelling ("'=>' after pattern").
    pub(crate) fn expect_as<K: TokenType>(&mut self, expected: &str) -> Result<Token<K>, Missing> {
        self.cur.eat::<K>().ok_or_else(|| self.missing(expected))
    }

    /// Report that `expected` is not here; the [`Missing`] for the position.
    pub(crate) fn missing(&mut self, expected: impl Into<String>) -> Missing {
        let error = self.unexpected_error(expected);
        self.out.missing(error)
    }

    /// One level deeper into a nested construct named `what`; past
    /// [`MAX_NESTING`] the construct is reported, the bracket group at the
    /// cursor skipped, and the reported [`Missing`] returned for its place.
    /// Every `Ok` is paired with a [`Parser::leave_nesting`].
    pub(crate) fn enter_nesting(&mut self, what: &str) -> Result<(), Missing> {
        if self.depth >= MAX_NESTING {
            let range = self.cur.error_range();
            let missing = self.missing_because(
                format!("{what} nested more than {MAX_NESTING} levels deep"),
                range,
            );
            self.cur.skip_group();
            return Err(missing);
        }
        self.depth += 1;
        Ok(())
    }

    pub(crate) fn leave_nesting(&mut self) {
        self.depth -= 1;
    }

    /// A [`Missing`] explained by `message` rather than by "expected X".
    pub(crate) fn missing_because(
        &mut self,
        message: impl Into<String>,
        range: TextRange,
    ) -> Missing {
        self.out.missing(ParseError::InvalidSyntax {
            message: message.into(),
            range,
        })
    }

    // ----- bogus -----------------------------------------------------------------------

    /// The current token as a `Bogus`, reported as unexpected.
    pub(crate) fn bogus_one(&mut self, expected: impl Into<String>) -> Id<Bogus> {
        let error = self.unexpected_error(expected);
        let mark = self.cur.mark();
        self.cur.skip();
        let tokens = self.cur.range_from(mark);
        self.out.bogus(tokens, error)
    }

    /// The tokens consumed since `mark` as a `Bogus`, with their report.
    pub(crate) fn bogus_since(&mut self, mark: Mark, error: ParseError) -> Id<Bogus> {
        let tokens = self.cur.range_from(mark);
        self.out.bogus(tokens, error)
    }

    /// `tokens`, already consumed, as a `Bogus` with their report — for a run
    /// decided on after more was read past it.
    pub(crate) fn bogus_tokens(&mut self, tokens: TokenRange, error: ParseError) -> Id<Bogus> {
        debug_assert!(tokens.end() <= self.cur.end(), "`tokens` were consumed");
        self.out.bogus(tokens, error)
    }

    // ----- speculation -----------------------------------------------------------------

    /// Parse with `f`, deciding afterwards: the nodes it allocated are kept on
    /// `Ok` and discarded on `Err`. The tokens it consumed stay consumed either
    /// way (the parse decides after reading, it does not re-read) and the
    /// diagnostics it reported stay reported; the `Err` is for the caller to
    /// put a `Bogus` over the same tokens. Attempts nest.
    pub(crate) fn attempt<T, E>(
        &mut self,
        f: impl FnOnce(&mut Parser<'t, '_>) -> Result<T, E>,
    ) -> Result<T, E> {
        let cur = self.cur;
        let depth = self.depth;
        let (cur, result) = self.out.speculate(
            |out| {
                let mut child = Parser { cur, out, depth };
                let result = f(&mut child);
                (child.cur, result)
            },
            |(_, result)| result.is_ok(),
        );
        // What the child read stays read: a speculation decides the nodes,
        // never the input.
        self.cur = cur;
        result
    }

    /// Parse with `f` for its tokens and diagnostics only: the nodes it
    /// allocated are discarded.
    pub(crate) fn discard(&mut self, f: impl FnOnce(&mut Parser<'t, '_>)) {
        let _discarded: Result<(), ()> = self.attempt(|p| {
            f(p);
            Err(())
        });
    }

    /// Parse with `f` for its tokens only: the nodes it allocated *and* the
    /// diagnostics it reported are discarded. For text read to find where it
    /// ends, when a diagnostic of the caller's already covers all of it.
    pub(crate) fn probe<R>(&mut self, f: impl FnOnce(&mut Parser<'t, '_>) -> R) -> R {
        let cur = self.cur;
        let depth = self.depth;
        let (cur, result) = self.out.probe(|out| {
            let mut child = Parser { cur, out, depth };
            let result = f(&mut child);
            (child.cur, result)
        });
        self.cur = cur;
        result
    }

    // ----- lists -----------------------------------------------------------------------

    /// `first (sep item)*`, `first` already parsed: a separator continues the
    /// list while `item` produces something (`None`, after its own diagnostic,
    /// ends it — the separator stays as a trailing one). A list that must not
    /// be empty.
    pub(crate) fn sep1_list<T, S: TokenType>(
        &mut self,
        first: T,
        mut item: impl FnMut(&mut Self) -> Option<T>,
    ) -> Sep1<T, S>
    where
        Arenas: HasPool<SepItem<T, S>>,
        SepItem<T, S>: PoolItem,
    {
        let mut items = vec![SepItem {
            item: first,
            sep: None,
        }];
        while let Some(sep) = self.cur.eat::<S>() {
            if let Some(last) = items.last_mut() {
                last.sep = Some(sep);
            }
            match item(self) {
                Some(next) => items.push(SepItem {
                    item: next,
                    sep: None,
                }),
                None => break,
            }
        }
        let mut rest = items.into_iter();
        let first = rest
            .next()
            .unwrap_or_else(|| unreachable!("started with one item"));
        self.out.alloc_list1(first, rest.collect())
    }

    /// `item (sep item)*` up to `close`, a trailing separator allowed. An item
    /// that cannot start (`None`, after its own diagnostic) is skipped up to
    /// the next separator or `close`.
    pub(crate) fn sep_list<T, S: TokenType>(
        &mut self,
        close: TokenKind,
        mut item: impl FnMut(&mut Self) -> Option<T>,
    ) -> Sep<T, S>
    where
        Arenas: HasPool<SepItem<T, S>>,
        SepItem<T, S>: PoolItem,
    {
        let mut items = Vec::new();
        if !self.cur.at(close) {
            loop {
                let parsed = item(self);
                if parsed.is_none() {
                    // Angle brackets nest only in a type list; in an argument
                    // or parameter list `<` is a comparison.
                    let angles_nest = close == TokenKind::Greater;
                    self.cur
                        .skip_until_in(KindSet::new(&[close, S::KIND]), angles_nest);
                }
                let sep = self.cur.eat::<S>();
                let more = sep.is_some() && !self.cur.at(close) && !self.cur.at_end();
                if let Some(item) = parsed {
                    items.push(SepItem { item, sep });
                }
                if !more {
                    break;
                }
            }
        }
        self.out.alloc_list(items)
    }

    /// The members of a `{ … }`/`[ … ]` body up to `close`, each parsed with
    /// its own trailing separator by `member`, which returns the member and
    /// — when it found no separator after it — the message for one being
    /// required there ("',' or '}' after …"), reported unless `close`
    /// follows; after the report one token is skipped unless a member can
    /// begin at it (`resume`). A member that cannot start (`None`, after its
    /// own diagnostic) skips one token the same way. A token in `boundary`
    /// (a statement keyword) ends the body early: its closer was forgotten.
    pub(crate) fn members<T>(
        &mut self,
        close: TokenKind,
        boundary: KindSet,
        resume: KindSet,
        mut member: impl FnMut(&mut Self) -> Option<(T, Option<&'static str>)>,
    ) -> Vec<T> {
        let mut items = Vec::new();
        while !self.cur.at(close) && !self.cur.at_end() && !self.cur.at_set(boundary) {
            let before = self.cur.pos();
            match member(self) {
                Some((item, None)) => {
                    items.push(item);
                    continue;
                }
                Some((item, Some(missing_sep))) => {
                    items.push(item);
                    if self.cur.at(close) || self.cur.at_end() || self.cur.at_set(boundary) {
                        break;
                    }
                    self.unexpected(missing_sep);
                }
                None => {
                    if self.cur.at(close) || self.cur.at_end() || self.cur.at_set(boundary) {
                        break;
                    }
                }
            }
            // A member that consumed nothing must not be tried again at the
            // same token: a token in `resume` that no member can start at
            // (`<` is an expression start only as a generic lambda) is
            // skipped all the same, or the loop never ends.
            if !self.cur.at_set(resume) || self.cur.pos() == before {
                self.cur.skip();
            }
        }
        items
    }
}

/// The name of a token kind in "expected …, found …" diagnostics.
///
/// The wording is the rowan parser's token names, kept so the diagnostics
/// stay byte-identical across the parser cutover (the parity oracle compares
/// them): keywords by spelling, punctuation quoted, every kind of name
/// "identifier", a string by its opening quote. Revisited with the diagnostic
/// rework.
pub(crate) fn found_name(kind: TokenKind) -> String {
    use TokenKind as K;
    let fixed = match kind {
        K::Word | K::RawString | K::UnclosedRawString | K::ByteString | K::UnclosedByteString => {
            "identifier"
        }
        K::Integer => "integer",
        K::Float => "float",
        K::Bigint => "bigint",
        K::String | K::UnclosedString => "'\"'",
        K::LegacyHashString | K::UnclosedHashString | K::Hash => "'#'",
        K::BacktickStart | K::BacktickEnd => "'`'",
        K::DollarLBrace => "'$'",
        K::Error => "error",
        kind if kind.is_contextual_keyword() => "identifier",
        kind if kind.is_punct() => return format!("'{}'", kind.describe()),
        kind => kind.describe(),
    };
    fixed.to_string()
}
