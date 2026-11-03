//! Where the parse goes: the tree under construction and the diagnostics.
//!
//! The two recovery values have exactly one way in each. A [`Missing`] comes
//! from [`Sink::missing`] and a [`Bogus`] from [`Sink::bogus`], and both take
//! the diagnostic they belong to; the tree's [`Builder`] enforces that, so
//! neither can exist in the tree without its report. (The grammar layers not
//! yet ported use [`Sink::placeholder`]; it is deleted with them.)
//!
//! A sink borrows its storage. A speculative parse ([`Sink::speculate`])
//! gets a child sink over a speculation on the same arenas, so the parent is
//! unreachable until the child is done and the borrow checker orders nested
//! speculations.

use baml_syntax::{
    nodes::{Arenas, Builder},
    tree::{
        AnyNodeId, Bogus, HasArena, HasPool, Id, List, List1, Missing, NodeMarker, PoolItem,
        TokenRange,
    },
};

use crate::error::ParseError;

pub(crate) struct Sink<'a> {
    builder: Builder<'a, ParseError>,
    /// Ranges the parser has not been taught to read yet (see
    /// [`Sink::placeholder`]).
    placeholders: &'a mut Vec<TokenRange>,
}

impl<'a> Sink<'a> {
    pub(crate) fn new(
        arenas: &'a mut Arenas,
        errors: &'a mut Vec<ParseError>,
        placeholders: &'a mut Vec<TokenRange>,
    ) -> Self {
        Self {
            builder: Builder::new(arenas, errors),
            placeholders,
        }
    }

    // ----- nodes ----------------------------------------------------------------

    pub(crate) fn alloc<T: NodeMarker>(&mut self, node: T, span: TokenRange) -> Id<T>
    where
        Arenas: HasArena<T>,
    {
        self.builder.alloc(node, span)
    }

    pub(crate) fn alloc_list<T: PoolItem>(&mut self, items: Vec<T>) -> List<T>
    where
        Arenas: HasPool<T>,
    {
        self.builder.alloc_list(items)
    }

    /// A list that must not be empty: `first`, then `rest`.
    pub(crate) fn alloc_list1<T: PoolItem>(&mut self, first: T, rest: Vec<T>) -> List1<T>
    where
        Arenas: HasPool<T>,
    {
        self.builder.alloc_list1(first, rest)
    }

    /// Run `f` with a sink for a speculative parse: what it allocates is kept
    /// when `keep` says so of the result and discarded otherwise. The
    /// diagnostics it reports stay either way.
    pub(crate) fn speculate<R>(
        &mut self,
        f: impl FnOnce(Sink<'_>) -> R,
        keep: impl FnOnce(&R) -> bool,
    ) -> R {
        let placeholders = &mut *self.placeholders;
        self.builder.speculate(
            |builder| {
                f(Sink {
                    builder,
                    placeholders,
                })
            },
            keep,
        )
    }

    pub(crate) fn span(&self, id: AnyNodeId) -> TokenRange {
        self.builder.arenas().span(id)
    }

    /// The tree built so far, for a pass that reads back what was parsed.
    pub(crate) fn arenas(&self) -> &Arenas {
        self.builder.arenas()
    }

    /// The node behind a typed id.
    pub(crate) fn get<T: NodeMarker>(&self, id: Id<T>) -> &T
    where
        Arenas: HasArena<T>,
    {
        self.builder.arenas().get(id)
    }

    /// The token span of a typed id.
    pub(crate) fn node_span<T: NodeMarker>(&self, id: Id<T>) -> TokenRange
    where
        Arenas: HasArena<T>,
    {
        self.builder.arenas().node_span(id)
    }

    /// The items of a list.
    pub(crate) fn list<T: PoolItem>(&self, list: List<T>) -> &[T]
    where
        Arenas: HasPool<T>,
    {
        self.builder.arenas().list(list)
    }

    /// The spans of every `Bogus` so far.
    pub(crate) fn bogus_spans(&self) -> Vec<TokenRange> {
        let arenas = self.builder.arenas();
        arenas
            .ids::<Bogus>()
            .map(|id| arenas.node_span(id))
            .collect()
    }

    // ----- diagnostics ------------------------------------------------------------

    pub(crate) fn report(&mut self, error: ParseError) {
        self.builder.report(error);
    }

    /// A required position the text did not fill, with its report.
    pub(crate) fn missing(&mut self, error: ParseError) -> Missing {
        let at = error.range().start();
        self.builder.missing(at, error)
    }

    /// Tokens that fit no node, with their report.
    pub(crate) fn bogus(&mut self, tokens: TokenRange, error: ParseError) -> Id<Bogus> {
        self.builder.bogus(tokens, error)
    }

    /// A `Bogus` whose report lives elsewhere: until the last grammar layer
    /// lands, a construct the parser cannot read yet (the stubs).
    #[deprecated = "a placeholder for a layer not parsed yet; deleted with the last stub"]
    #[expect(
        deprecated,
        reason = "the builder's placeholder is the same transitional entry"
    )]
    pub(crate) fn placeholder(&mut self, tokens: TokenRange) -> Id<Bogus> {
        self.placeholders.push(tokens);
        self.builder.placeholder(tokens)
    }
}
