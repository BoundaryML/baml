//! The VM's memo of impl-resolver answers.
//!
//! [`ImplResolver`](crate::package_baml::ImplResolver) answers "which impl of
//! interface `I` does type `T` use" by scanning every rule of `I`, walking the
//! runtime package graph and proving bounds. `==` on classes and enums,
//! `${...}` interpolation, json decode and encode, hashing and interface field
//! reads ask that question on every call, so each VM keeps the answers here.
//!
//! An answer depends on three things: the question (the type, the interface
//! and its arguments), the world the resolver searches (its root, additional
//! and lexical runtime packages), and the rules that world holds. The key
//! holds the first two. The rules change only at a few points, and each one
//! keeps the cache exact:
//!
//! - Registering a witness and rewriting a package's rule tables (graft
//!   `publish`) bump the engine-wide generation in
//!   [`DynDispatchTables`](crate::package_load::DynDispatchTables). A lookup
//!   that reads a new generation drops every entry first.
//! - A garbage collection moves heap objects and frees dead ones.
//!   [`ImplCache::visit_weak`] rewrites every pointer the entries hold and drops
//!   each entry that refers to a freed object. Entries are weak: they keep
//!   nothing alive.
//!
//! Each VM (one per `BexThread`) owns its cache, so a lookup takes no lock:
//! one atomic load of the generation, then a hash lookup. An answer whose
//! computation overlapped a generation bump is not stored.

use std::hash::{Hash, Hasher};

use baml_type::{Name, StructuralInterface};
use bex_vm_types::{HeapPtr, RealizedTy, RootVisitor, TypeHead};
use indexmap::{Equivalent, IndexMap};
use rustc_hash::FxBuildHasher;

/// A table holding more entries than this is emptied before the next insert.
/// That bounds a VM that meets an unbounded stream of types, such as generic
/// instantiations or classes minted in a loop.
const CAPACITY: usize = 4096;

/// The dynamic world a resolver searches: the package its candidate walk
/// starts from (null when no runtime package is involved), the package it
/// also walks, and the package of the running code. The last one matters
/// even when the walk skips it: substituting a rule's templates reduces
/// associated-type projections through `BexVm::project`, which searches the
/// running code's package.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct World {
    pub(crate) root: HeapPtr,
    pub(crate) additional: Option<HeapPtr>,
    pub(crate) lexical: HeapPtr,
}

/// One resolver question, borrowing its types. Lookups hash this form, so a
/// hit allocates nothing.
#[derive(Debug, PartialEq, Hash)]
pub(crate) struct Query<'a> {
    pub(crate) world: World,
    /// The receiver's concrete base type: a literal or enum-variant type is
    /// asked as its base, which is all the resolver reads.
    pub(crate) self_ty: &'a RealizedTy,
    pub(crate) interface: TypeHead,
    pub(crate) args: &'a [RealizedTy],
    /// The associated bindings a membership question pins; empty otherwise.
    pub(crate) assoc: &'a [(Name, RealizedTy)],
}

impl Query<'_> {
    fn to_key(&self) -> Key {
        Key {
            world: self.world,
            self_ty: self.self_ty.clone(),
            interface: self.interface,
            args: self.args.into(),
            assoc: self.assoc.into(),
        }
    }
}

/// The owned form of a [`Query`]. It hashes and compares through
/// [`Key::as_query`], so the two forms always agree.
#[derive(Debug)]
pub(crate) struct Key {
    world: World,
    self_ty: RealizedTy,
    interface: TypeHead,
    args: Box<[RealizedTy]>,
    assoc: Box<[(Name, RealizedTy)]>,
}

impl Key {
    fn as_query(&self) -> Query<'_> {
        Query {
            world: self.world,
            self_ty: &self.self_ty,
            interface: self.interface,
            args: &self.args,
            assoc: &self.assoc,
        }
    }

    /// Follow a collection; `false` when the key names a freed object.
    fn forward(&mut self, follow: &mut impl FnMut(&mut HeapPtr) -> bool) -> bool {
        let mut live = follow(&mut self.world.root);
        live &= follow(&mut self.world.lexical);
        if let Some(additional) = &mut self.world.additional {
            live &= follow(additional);
        }
        live &= forward_head(&mut self.interface, follow);
        live &= forward_ty(&mut self.self_ty, follow);
        for arg in &mut self.args {
            live &= forward_ty(arg, follow);
        }
        for (_, ty) in &mut self.assoc {
            live &= forward_ty(ty, follow);
        }
        live
    }
}

impl Hash for Key {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_query().hash(state);
    }
}

impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        self.as_query() == other.as_query()
    }
}

impl Eq for Key {}

impl Equivalent<Key> for Query<'_> {
    fn equivalent(&self, key: &Key) -> bool {
        *self == key.as_query()
    }
}

pub(crate) type Entries<V> = IndexMap<Key, V, FxBuildHasher>;

/// The explicit rule a question resolved to: the heap `Object::ImplRule`,
/// whether it lives in the static program image, and the impl's bound type
/// arguments.
#[derive(Clone, Debug)]
pub(crate) struct CachedRule {
    pub(crate) rule: HeapPtr,
    pub(crate) is_static: bool,
    pub(crate) bindings: Vec<RealizedTy>,
}

impl CachedRule {
    fn forward(&mut self, follow: &mut impl FnMut(&mut HeapPtr) -> bool) -> bool {
        let mut live = follow(&mut self.rule);
        for ty in &mut self.bindings {
            live &= forward_ty(ty, follow);
        }
        live
    }
}

/// What `resolve_implementation` selected: an explicit rule or the
/// interface's structural default.
#[derive(Clone, Debug)]
pub(crate) enum CachedImplementation {
    Explicit(CachedRule),
    Structural(StructuralInterface),
}

/// A value the cache stores: the answer of one resolver entry point.
pub(crate) trait Answer: Clone {
    /// The table this kind of answer lives in.
    fn entries(cache: &mut ImplCache) -> &mut Entries<Self>;

    /// Follow a collection; `false` when the answer names a freed object.
    fn forward(&mut self, follow: &mut impl FnMut(&mut HeapPtr) -> bool) -> bool;
}

/// `resolve_implements_rule`: the applicable explicit rule, if any.
impl Answer for Option<CachedRule> {
    fn entries(cache: &mut ImplCache) -> &mut Entries<Self> {
        &mut cache.rules
    }

    fn forward(&mut self, follow: &mut impl FnMut(&mut HeapPtr) -> bool) -> bool {
        self.as_mut().is_none_or(|rule| rule.forward(follow))
    }
}

/// `resolve_implementation`: the explicit rule or structural default, if any.
impl Answer for Option<CachedImplementation> {
    fn entries(cache: &mut ImplCache) -> &mut Entries<Self> {
        &mut cache.implementations
    }

    fn forward(&mut self, follow: &mut impl FnMut(&mut HeapPtr) -> bool) -> bool {
        match self {
            Some(CachedImplementation::Explicit(rule)) => rule.forward(follow),
            Some(CachedImplementation::Structural(_)) | None => true,
        }
    }
}

/// `type_implements`: whether the type implements the interface.
impl Answer for bool {
    fn entries(cache: &mut ImplCache) -> &mut Entries<Self> {
        &mut cache.memberships
    }

    fn forward(&mut self, _follow: &mut impl FnMut(&mut HeapPtr) -> bool) -> bool {
        true
    }
}

/// One VM's cached resolver answers. See the module docs.
#[derive(Default)]
pub(crate) struct ImplCache {
    /// The engine generation every entry was computed under.
    generation: u64,
    rules: Entries<Option<CachedRule>>,
    implementations: Entries<Option<CachedImplementation>>,
    memberships: Entries<bool>,
}

impl ImplCache {
    /// The stored answer to `query`. When the engine's generation is no
    /// longer the one the entries were computed under, every entry is
    /// dropped first.
    pub(crate) fn get<V: Answer>(&mut self, generation: u64, query: &Query<'_>) -> Option<&V> {
        if self.generation != generation {
            self.rules.clear();
            self.implementations.clear();
            self.memberships.clear();
            self.generation = generation;
        }
        V::entries(self).get(query)
    }

    /// Store `answer`, computed by a resolution that started at generation
    /// `started` and ended at `now`. If the generation moved in between, the
    /// rules may have changed under the computation, so nothing is stored.
    pub(crate) fn insert<V: Answer>(
        &mut self,
        started: u64,
        now: u64,
        query: &Query<'_>,
        answer: V,
    ) {
        if started != now || self.generation != started {
            return;
        }
        let entries = V::entries(self);
        if entries.len() >= CAPACITY {
            entries.clear();
        }
        entries.insert(query.to_key(), answer);
    }

    /// Walk the entries for a collection: repoint every heap pointer they
    /// hold, and drop each entry that names an object the collection freed.
    /// The cache is not a root, so every pointer is visited weakly; a null
    /// package (no runtime package involved) names nothing.
    pub(crate) fn visit_weak(&mut self, visitor: &mut dyn RootVisitor) {
        let mut follow = |ptr: &mut HeapPtr| ptr.is_null() || visitor.visit_weak(ptr);
        forward_entries(&mut self.rules, &mut follow);
        forward_entries(&mut self.implementations, &mut follow);
        forward_entries(&mut self.memberships, &mut follow);
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.rules.len() + self.implementations.len() + self.memberships.len()
    }

    #[cfg(test)]
    pub(crate) fn cached_rules(&self) -> impl Iterator<Item = HeapPtr> + '_ {
        self.rules.values().flatten().map(|rule| rule.rule).chain(
            self.implementations.values().flatten().filter_map(
                |implementation| match implementation {
                    CachedImplementation::Explicit(rule) => Some(rule.rule),
                    CachedImplementation::Structural(_) => None,
                },
            ),
        )
    }
}

/// Rebuild `entries` through a collection. The keys hash their package
/// pointers, so a moved package changes its entry's slot.
fn forward_entries<V: Answer>(
    entries: &mut Entries<V>,
    follow: &mut impl FnMut(&mut HeapPtr) -> bool,
) {
    if entries.is_empty() {
        return;
    }
    let old = std::mem::take(entries);
    for (mut key, mut answer) in old {
        if key.forward(follow) && answer.forward(follow) {
            entries.insert(key, answer);
        }
    }
}

fn forward_head(head: &mut TypeHead, follow: &mut impl FnMut(&mut HeapPtr) -> bool) -> bool {
    if !head.is_resolved() {
        return true;
    }
    let mut ptr = head.ptr();
    let live = follow(&mut ptr);
    head.forward_to(ptr);
    live
}

fn forward_ty(ty: &mut RealizedTy, follow: &mut impl FnMut(&mut HeapPtr) -> bool) -> bool {
    let mut live = true;
    ty.visit_heads_mut(&mut |head| live &= forward_head(head, follow));
    live
}
