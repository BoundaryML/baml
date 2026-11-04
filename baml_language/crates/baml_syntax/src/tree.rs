//! Arena storage and the traits the derive macros implement.

use std::{
    marker::PhantomData,
    num::NonZeroU32,
    ops::{Index, Range},
};

use text_size::TextSize;

use crate::token::{Token, TokenIdx, TokenSet};

// ---------------------------------------------------------------------------
// Ids and positions
// ---------------------------------------------------------------------------

/// A node of type `T`, by index into its arena. `T` is a node type: anything
/// else has no arena to index.
pub struct Id<T: NodeMarker> {
    /// Position in the arena for `T`.
    raw: u32,
    ty: PhantomData<fn() -> T>,
}

impl<T: NodeMarker> Id<T> {
    pub(crate) fn from_raw(raw: u32) -> Self {
        Self {
            raw,
            ty: PhantomData,
        }
    }

    pub fn raw(self) -> u32 {
        self.raw
    }
}

impl<T: NodeMarker> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: NodeMarker> Copy for Id<T> {}
impl<T: NodeMarker> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}
impl<T: NodeMarker> Eq for Id<T> {}
impl<T: NodeMarker> PartialOrd for Id<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<T: NodeMarker> Ord for Id<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.raw.cmp(&other.raw)
    }
}
impl<T: NodeMarker> std::hash::Hash for Id<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.raw.hash(state);
    }
}
impl<T: NodeKindOf> std::fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}#{}", T::NAME, self.raw)
    }
}

/// A half-open range of tokens, `start <= end` by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TokenRange {
    start: TokenIdx,
    end: TokenIdx,
}

impl TokenRange {
    pub fn new(start: TokenIdx, end: TokenIdx) -> Self {
        debug_assert!(start <= end, "a token range runs forward");
        Self { start, end }
    }

    pub fn start(self) -> TokenIdx {
        self.start
    }

    /// One past the last token.
    pub fn end(self) -> TokenIdx {
        self.end
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    pub fn as_range(self) -> Range<TokenIdx> {
        self.start..self.end
    }

    /// Every token index in the range.
    pub fn iter(self) -> impl Iterator<Item = TokenIdx> {
        (self.start.0..self.end.0).map(TokenIdx)
    }
}

/// A raw range in a node (`Bogus`, a removed item's head and body, a jinja
/// body) is reported as one [`Visitor::range`]: the node covers those tokens
/// without reading them, trivia included.
impl<A> Walk<A> for TokenRange {
    fn walk(&self, _arena: &A, field: &'static str, visitor: &mut dyn Visitor) {
        visitor.range(field, *self);
    }
}

/// A required child the parser could not produce. The position is where it
/// should have begun — a text offset rather than a token, because it is a
/// zero-width caret that may fall between tokens or at the end of the text;
/// a diagnostic was emitted when this was created, and the tree builder is
/// the only way to make one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Missing {
    at: TextSize,
}

impl Missing {
    pub(crate) fn new(at: TextSize) -> Self {
        Self { at }
    }

    /// Where the child should have begun.
    pub fn at(self) -> TextSize {
        self.at
    }
}

// ---------------------------------------------------------------------------
// Node identity across types
// ---------------------------------------------------------------------------

/// A node of any type: its kind (as declared by `define_tree!`) and index.
/// The one place a node is addressed by plain numbers, so that a generic walk
/// can hold ids of different types; [`cast`](Self::cast) recovers the typed
/// id. Ordered by kind, then by allocation index — a stable key for sets and
/// maps, not source order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AnyNodeId {
    /// The node type's position in the `define_tree!` list (`NodeKindOf::RAW`).
    kind: u16,
    /// The [`Id`]'s position in that type's arena.
    raw: u32,
}

impl AnyNodeId {
    pub fn of<T: NodeKindOf>(id: Id<T>) -> Self {
        Self {
            kind: T::RAW,
            raw: id.raw,
        }
    }

    /// An id from its parts, for an index that stores ids as plain numbers.
    /// Only a kind and index read off a real id go back in.
    pub(crate) const fn from_raw_parts(kind: u16, raw: u32) -> Self {
        Self { kind, raw }
    }

    pub fn kind_raw(self) -> u16 {
        self.kind
    }

    pub fn raw(self) -> u32 {
        self.raw
    }

    /// The typed id, if this node is a `T`.
    pub fn cast<T: NodeKindOf>(self) -> Option<Id<T>> {
        (self.kind == T::RAW).then(|| Id::from_raw(self.raw))
    }
}

/// Only this crate's derives implement the node traits: a type outside it
/// cannot claim a kind (and forge typed ids through `cast`) or an arena.
pub(crate) mod sealed {
    pub trait Sealed {}
}

/// Implemented by `define_tree!` for every listed node type: its kind. Every
/// listed type is a node, so this implies [`NodeMarker`].
pub trait NodeKindOf: NodeMarker + 'static {
    const RAW: u16;
    const NAME: &'static str;
}

/// Implemented by `#[derive(Node)]`: this struct is a node. `NodeArena<T>`
/// requires it, and [`NodeKindOf`] (from `define_tree!`) is what `Id<T>`
/// needs to be walked — so a derived-but-unregistered struct cannot appear in
/// a tree, and a registered-but-underived type cannot have an arena. Both are
/// compile errors at the first use.
pub trait NodeMarker: sealed::Sealed {}

/// Implemented by `define_tree!` for every listed pool type: what a
/// [`List<T>`] can hold. Anything else has no pool to index.
pub trait PoolItem: sealed::Sealed {}

// ---------------------------------------------------------------------------
// Walking
// ---------------------------------------------------------------------------

/// Receives a node's parts in source order.
pub trait Visitor {
    fn node(&mut self, field: &'static str, id: AnyNodeId);
    /// A token the node owns.
    fn token(&mut self, field: &'static str, token: TokenIdx);
    /// The separator after an item of a separated list — a token the node
    /// owns, told apart from the items for walks that check list shape.
    fn separator(&mut self, field: &'static str, token: TokenIdx) {
        self.token(field, token);
    }
    /// An item of a separated list with no separator after it: the last
    /// item, in a list that parsed cleanly.
    fn unseparated(&mut self, _field: &'static str) {}
    /// A run of tokens the node covers without reading them — a `Bogus`, a
    /// removed item's head and body, a jinja body. Trivia inside it counts.
    fn range(&mut self, field: &'static str, range: TokenRange);
    fn missing(&mut self, field: &'static str, missing: Missing);
}

/// A value in a field position: visits what it refers to. Generic over the
/// arena type so `List<T>` can read its pool.
pub trait Walk<A> {
    fn walk(&self, arena: &A, field: &'static str, visitor: &mut dyn Visitor);
}

/// A node's own fields, in declaration order. Derived by `#[derive(Node)]`.
pub trait NodeFields<A> {
    fn walk_fields(&self, arena: &A, visitor: &mut dyn Visitor);
}

/// A union of alternatives (`#[derive(AnyNode)]`).
pub trait AnyNode {
    /// The node this alternative refers to, if it is a node (a variant may
    /// instead hold a token).
    fn any_node_id(&self) -> Option<AnyNodeId>;
}

/// What an `AnyNode` payload contributes to [`AnyNode::any_node_id`].
pub trait AnyNodeIdOf {
    fn any_node_id(&self) -> Option<AnyNodeId>;
}

impl<T: NodeKindOf> AnyNodeIdOf for Id<T> {
    fn any_node_id(&self) -> Option<AnyNodeId> {
        Some(AnyNodeId::of(*self))
    }
}

impl<S: TokenSet> AnyNodeIdOf for Token<S> {
    fn any_node_id(&self) -> Option<AnyNodeId> {
        None
    }
}

/// Generic access to a tree's nodes, implemented by the generated `Arenas`.
pub trait TreeWalk {
    fn kind_name(&self, id: AnyNodeId) -> &'static str;
    fn span(&self, id: AnyNodeId) -> TokenRange;
    fn walk_node(&self, id: AnyNodeId, visitor: &mut dyn Visitor);
}

impl<A, T: NodeKindOf> Walk<A> for Id<T> {
    fn walk(&self, _arena: &A, field: &'static str, visitor: &mut dyn Visitor) {
        visitor.node(field, AnyNodeId::of(*self));
    }
}

impl<A, S: TokenSet> Walk<A> for Token<S> {
    fn walk(&self, _arena: &A, field: &'static str, visitor: &mut dyn Visitor) {
        visitor.token(field, self.idx);
    }
}

impl<A, W: Walk<A>> Walk<A> for Option<W> {
    fn walk(&self, arena: &A, field: &'static str, visitor: &mut dyn Visitor) {
        if let Some(inner) = self {
            inner.walk(arena, field, visitor);
        }
    }
}

impl<A, W: Walk<A>> Walk<A> for Result<W, Missing> {
    fn walk(&self, arena: &A, field: &'static str, visitor: &mut dyn Visitor) {
        match self {
            Ok(inner) => inner.walk(arena, field, visitor),
            Err(missing) => visitor.missing(field, *missing),
        }
    }
}

// ---------------------------------------------------------------------------
// Lists
// ---------------------------------------------------------------------------

/// A position in the arenas' pool for `T`: where a [`List<T>`] begins.
pub struct PoolIdx<T: PoolItem> {
    raw: u32,
    ty: PhantomData<fn() -> T>,
}

impl<T: PoolItem> PoolIdx<T> {
    pub(crate) fn from_raw(raw: u32) -> Self {
        Self {
            raw,
            ty: PhantomData,
        }
    }

    pub fn index(self) -> usize {
        self.raw as usize
    }
}

impl<T: PoolItem> Clone for PoolIdx<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: PoolItem> Copy for PoolIdx<T> {}
impl<T: PoolItem> PartialEq for PoolIdx<T> {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}
impl<T: PoolItem> Eq for PoolIdx<T> {}
impl<T: PoolItem> std::hash::Hash for PoolIdx<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.raw.hash(state);
    }
}
impl<T: PoolItem> std::fmt::Debug for PoolIdx<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "pool#{}", self.raw)
    }
}

/// A contiguous run of `T`s in the arenas' pool for `T`. An empty list is a
/// zero-length run.
pub struct List<T: PoolItem> {
    /// The first item.
    start: PoolIdx<T>,
    /// How many items follow it.
    len: u32,
}

impl<T: PoolItem> List<T> {
    pub(crate) fn from_raw(start: PoolIdx<T>, len: u32) -> Self {
        Self { start, len }
    }

    pub fn start(self) -> PoolIdx<T> {
        self.start
    }

    pub fn len(self) -> usize {
        self.len as usize
    }

    pub fn is_empty(self) -> bool {
        self.len == 0
    }

    /// The items, read from the arenas that hold the pool.
    pub fn get<A: HasPool<T>>(self, arena: &A) -> &[T] {
        &arena.pool()[self.start.index()..self.start.index() + self.len()]
    }
}

impl<T: PoolItem> Clone for List<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: PoolItem> Copy for List<T> {}
impl<T: PoolItem> PartialEq for List<T> {
    fn eq(&self, other: &Self) -> bool {
        self.start == other.start && self.len == other.len
    }
}
impl<T: PoolItem> Eq for List<T> {}
impl<T: PoolItem> std::hash::Hash for List<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (self.start, self.len).hash(state);
    }
}
impl<T: PoolItem> std::fmt::Debug for List<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "List[{}..{}]", self.start.raw, self.start.raw + self.len)
    }
}

impl<A: HasPool<T>, T: PoolItem + Walk<A>> Walk<A> for List<T> {
    fn walk(&self, arena: &A, field: &'static str, visitor: &mut dyn Visitor) {
        for item in self.get(arena) {
            item.walk(arena, field, visitor);
        }
    }
}

/// A [`List`] with at least one item: a grammar position that needs one or
/// more items says so here instead of in a comment. Same run in the same
/// pool; the length is the non-zero kind.
pub struct List1<T: PoolItem> {
    start: PoolIdx<T>,
    len: NonZeroU32,
}

impl<T: PoolItem> List1<T> {
    pub(crate) fn from_raw(start: PoolIdx<T>, len: NonZeroU32) -> Self {
        Self { start, len }
    }

    pub fn start(self) -> PoolIdx<T> {
        self.start
    }

    pub fn len(self) -> NonZeroU32 {
        self.len
    }

    /// The same run as a plain list.
    pub fn as_list(self) -> List<T> {
        List::from_raw(self.start, self.len.get())
    }

    /// The items, read from the arenas that hold the pool; never empty.
    pub fn get<A: HasPool<T>>(self, arena: &A) -> &[T] {
        self.as_list().get(arena)
    }
}

impl<T: PoolItem> Clone for List1<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: PoolItem> Copy for List1<T> {}
impl<T: PoolItem> PartialEq for List1<T> {
    fn eq(&self, other: &Self) -> bool {
        self.start == other.start && self.len == other.len
    }
}
impl<T: PoolItem> Eq for List1<T> {}
impl<T: PoolItem> std::hash::Hash for List1<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (self.start, self.len).hash(state);
    }
}
impl<T: PoolItem> std::fmt::Debug for List1<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_list().fmt(f)
    }
}

impl<A: HasPool<T>, T: PoolItem + Walk<A>> Walk<A> for List1<T> {
    fn walk(&self, arena: &A, field: &'static str, visitor: &mut dyn Visitor) {
        self.as_list().walk(arena, field, visitor);
    }
}

/// A separator-delimited list with at least one item.
pub type Sep1<T, S> = List1<SepItem<T, S>>;

/// One item of a separated list and the separator that follows it, if any
/// (so a trailing separator is observable).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SepItem<T, S: TokenSet> {
    pub item: T,
    pub sep: Option<Token<S>>,
}

impl<A, T: Walk<A>, S: TokenSet> Walk<A> for SepItem<T, S> {
    fn walk(&self, arena: &A, field: &'static str, visitor: &mut dyn Visitor) {
        self.item.walk(arena, field, visitor);
        match self.sep {
            Some(sep) => visitor.separator(field, sep.idx),
            None => visitor.unseparated(field),
        }
    }
}

/// A separator-delimited list.
pub type Sep<T, S> = List<SepItem<T, S>>;

// ---------------------------------------------------------------------------
// Arenas
// ---------------------------------------------------------------------------

/// The nodes of one type and their token spans, in allocation order.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeArena<T: NodeMarker> {
    nodes: Vec<T>,
    spans: Vec<TokenRange>,
}

impl<T: NodeMarker> Default for NodeArena<T> {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            spans: Vec::new(),
        }
    }
}

impl<T: NodeMarker> NodeArena<T> {
    pub(crate) fn alloc(&mut self, node: T, span: TokenRange) -> Id<T> {
        let raw = u32::try_from(self.nodes.len())
            .unwrap_or_else(|_| unreachable!("a file never has 2^32 nodes of one kind"));
        self.nodes.push(node);
        self.spans.push(span);
        Id::from_raw(raw)
    }

    pub fn span(&self, id: Id<T>) -> TokenRange {
        self.spans[id.raw as usize]
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Bytes allocated for the nodes and their spans (capacity, not length:
    /// what the arena holds on the heap).
    pub fn heap_bytes(&self) -> usize {
        self.nodes.capacity() * size_of::<T>() + self.spans.capacity() * size_of::<TokenRange>()
    }

    /// Drop every node allocated at or after `len` (a speculation rolled
    /// back). `Id`s handed out for them are dangling afterwards; the caller
    /// guarantees none are referenced.
    pub(crate) fn truncate(&mut self, len: usize) {
        self.nodes.truncate(len);
        self.spans.truncate(len);
    }

    pub fn ids(&self) -> impl Iterator<Item = Id<T>> {
        (0..self.nodes.len()).map(|i| {
            Id::from_raw(u32::try_from(i).unwrap_or_else(|_| unreachable!("bounded by the arena")))
        })
    }
}

impl<T: NodeMarker> Index<Id<T>> for NodeArena<T> {
    type Output = T;
    fn index(&self, id: Id<T>) -> &T {
        &self.nodes[id.raw as usize]
    }
}

/// The arenas hold a `NodeArena<T>`. Implemented by `define_tree!`; the
/// mutable half is reachable only through `&mut Arenas`, which a finished
/// `FileTree` never hands out.
pub trait HasArena<T: NodeMarker> {
    fn arena(&self) -> &NodeArena<T>;
    fn arena_mut(&mut self) -> &mut NodeArena<T>;
}

/// The arenas hold a pool of `T` for `List<T>`. Implemented by `define_tree!`.
pub trait HasPool<T: PoolItem> {
    fn pool(&self) -> &[T];
    fn pool_mut(&mut self) -> &mut Vec<T>;
}

/// Append `items` to the pool and return the run they occupy.
pub(crate) fn alloc_list<A: HasPool<T>, T: PoolItem>(
    arenas: &mut A,
    items: impl IntoIterator<Item = T>,
) -> List<T> {
    let pool = arenas.pool_mut();
    let start = pool.len();
    pool.extend(items);
    let len = u32::try_from(pool.len() - start)
        .unwrap_or_else(|_| unreachable!("bounded by the pool length"));
    let start = PoolIdx::from_raw(
        u32::try_from(start).unwrap_or_else(|_| unreachable!("a file never has 2^32 list items")),
    );
    List::from_raw(start, len)
}

/// Append `first` and then `rest` to the pool and return the run they
/// occupy, non-empty by construction.
pub(crate) fn alloc_list1<A: HasPool<T>, T: PoolItem>(
    arenas: &mut A,
    first: T,
    rest: impl IntoIterator<Item = T>,
) -> List1<T> {
    let list = alloc_list(arenas, std::iter::once(first).chain(rest));
    let len = NonZeroU32::new(list.len).unwrap_or_else(|| unreachable!("`first` was appended"));
    List1::from_raw(list.start, len)
}

// ---------------------------------------------------------------------------
// The recovery node
// ---------------------------------------------------------------------------

/// Tokens the parser could not fit into any node. Every `Any*` union has a
/// `Bogus` variant, so garbage keeps its place without loss; it is only ever
/// created alongside a diagnostic — the tree builder is the only way to make
/// one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Bogus {
    tokens: TokenRange,
}

impl Bogus {
    pub(crate) fn new(tokens: TokenRange) -> Self {
        Self { tokens }
    }

    pub fn tokens(self) -> TokenRange {
        self.tokens
    }
}

impl sealed::Sealed for Bogus {}
impl NodeMarker for Bogus {}

impl<A> NodeFields<A> for Bogus {
    fn walk_fields(&self, arena: &A, visitor: &mut dyn Visitor) {
        self.tokens.walk(arena, "tokens", visitor);
    }
}
