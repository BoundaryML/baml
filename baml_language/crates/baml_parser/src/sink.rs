//! Where the parse goes: the tree under construction and the diagnostics.
//!
//! The two recovery values have exactly one way in each. A [`Missing`] comes
//! from [`Sink::missing`] and a [`Bogus`] from [`Sink::bogus`], and both take
//! the diagnostic they belong to; the tree's [`Builder`] enforces that, so
//! neither can exist in the tree without its report.
//!
//! A sink borrows its storage. A speculative parse ([`Sink::speculate`])
//! gets a child sink over a speculation on the same arenas, so the parent is
//! unreachable until the child is done and the borrow checker orders nested
//! speculations.

use baml_syntax::{
    nodes::{Arenas, Builder},
    token::TokenIdx,
    tree::{
        AnyNodeId, Bogus, HasArena, HasPool, Id, List, List1, Missing, NodeKindOf, NodeMarker,
        PoolItem, TokenRange, Visitor,
    },
};

use crate::error::ParseError;

pub(crate) struct Sink<'a> {
    builder: Builder<'a, ParseError>,
}

impl<'a> Sink<'a> {
    pub(crate) fn new(arenas: &'a mut Arenas, errors: &'a mut Vec<ParseError>) -> Self {
        Self {
            builder: Builder::new(arenas, errors),
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
        self.builder.speculate(|builder| f(Sink { builder }), keep)
    }

    /// Run `f` for the input it consumes only: its nodes and its diagnostics
    /// are both discarded (see `Builder::probe`).
    pub(crate) fn probe<R>(&mut self, f: impl FnOnce(Sink<'_>) -> R) -> R {
        self.builder.probe(|builder| f(Sink { builder }))
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

    /// The tokens every node of type `T` owns through its `Token<_>` fields
    /// so far (ranges and children excluded).
    pub(crate) fn owned_tokens<T: NodeKindOf>(&self) -> Vec<TokenIdx>
    where
        Arenas: HasArena<T>,
    {
        struct Owned(Vec<TokenIdx>);
        impl Visitor for Owned {
            fn node(&mut self, _field: &'static str, _id: AnyNodeId) {}
            fn token(&mut self, _field: &'static str, token: TokenIdx) {
                self.0.push(token);
            }
            fn range(&mut self, _field: &'static str, _range: TokenRange) {}
            fn missing(&mut self, _field: &'static str, _missing: Missing) {}
        }
        let arenas = self.builder.arenas();
        let mut owned = Owned(Vec::new());
        for id in arenas.ids::<T>() {
            arenas.walk_node(AnyNodeId::of(id), &mut owned);
        }
        owned.0
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
}
