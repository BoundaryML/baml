use ::std::{collections::HashMap, marker::PhantomData};

use crate::{HeapPtr, Value};

/// What a collection does at each root a [`RootHaver`] exposes: record it
/// (the root scan) or repoint it (forwarding after objects moved). One walk
/// serves both, so a holder names each root exactly once.
pub trait RootVisitor {
    /// A strong root: it keeps its object alive and is repointed when the
    /// object moves.
    fn visit(&mut self, root: &mut HeapPtr);

    /// A weak reference: repointed when its object moves, keeping nothing
    /// alive. Returns whether the object is still live after this collection;
    /// always `true` while roots are being collected, since nothing has died
    /// yet.
    fn visit_weak(&mut self, root: &mut HeapPtr) -> bool;
}

/// Whether a pointer absent from the forwarding map still names a live object.
///
/// Absence alone does not decide it: a compile-time pointer is never in the
/// map yet always live, and a pointer this forwarding pass already repointed
/// names the survivor space. Only the heap can tell those from the dead,
/// which is why [`ForwardRoots`] asks it rather than treating absence as
/// death.
pub trait Survival {
    fn survived(&self, ptr: HeapPtr) -> bool;
}

/// Nothing was collected, so every pointer is live: for a forwarding pass
/// that follows no collection, as tests run.
pub struct NothingCollected;

impl Survival for NothingCollected {
    fn survived(&self, _ptr: HeapPtr) -> bool {
        true
    }
}

/// The root scan: records every strong root, touches nothing.
pub struct CollectRoots<'a>(pub &'a mut Vec<HeapPtr>);

impl RootVisitor for CollectRoots<'_> {
    fn visit(&mut self, root: &mut HeapPtr) {
        self.0.push(*root);
    }

    fn visit_weak(&mut self, _root: &mut HeapPtr) -> bool {
        true
    }
}

/// Forwarding after a collection: repoints every root the collection moved
/// and answers liveness for weak references through the heap.
pub struct ForwardRoots<'a> {
    forwarding: &'a HashMap<HeapPtr, HeapPtr>,
    survival: &'a dyn Survival,
}

impl<'a> ForwardRoots<'a> {
    pub fn new(forwarding: &'a HashMap<HeapPtr, HeapPtr>, survival: &'a dyn Survival) -> Self {
        Self {
            forwarding,
            survival,
        }
    }
}

impl RootVisitor for ForwardRoots<'_> {
    fn visit(&mut self, root: &mut HeapPtr) {
        match self.forwarding.get(root) {
            Some(&moved) => *root = moved,
            None => debug_assert!(
                self.survival.survived(*root),
                "a strong root {root:?} the collection did not reach"
            ),
        }
    }

    fn visit_weak(&mut self, root: &mut HeapPtr) -> bool {
        if let Some(&moved) = self.forwarding.get(root) {
            *root = moved;
            true
        } else {
            self.survival.survived(*root)
        }
    }
}

/// A heap pointer held without keeping its object alive.
///
/// A cache or index entry that must follow its object when a collection moves
/// it, and must be dropped by its holder once the object is gone. Unlike a
/// strong root it is never reported to the collector, so it cannot resurrect
/// anything; [`Self::visit`] is how a holder learns the verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Weak(HeapPtr);

impl Weak {
    #[inline]
    pub fn new(ptr: HeapPtr) -> Self {
        Self(ptr)
    }

    /// The pointer. Valid while the object is live and has not moved: the
    /// `Weak` must sit in a holder the collector visits, its last
    /// [`Self::visit`] must have reported the object live, and the reader must
    /// hold a heap permit so no collection runs meanwhile. A `Weak` copied out
    /// of its holder goes stale at the next collection.
    #[inline]
    pub fn ptr(self) -> HeapPtr {
        self.0
    }

    /// Repoint for a collection. `false` once the object is gone; the holder
    /// must then drop this reference rather than read it.
    #[must_use]
    pub fn visit(&mut self, visitor: &mut dyn RootVisitor) -> bool {
        visitor.visit_weak(&mut self.0)
    }
}

/// A holder of heap roots: anything that keeps heap pointers outside the heap
/// itself and must therefore be scanned and repointed by the collector.
///
/// `Send` is required because instances are tracked across tokio tasks via the heap
/// permit manager. `Sync` is intentionally not required — the permit/mutex enforces
/// that at most one thread accesses the inner value at a time.
pub trait RootHaver: Send {
    /// Hand every heap pointer this value holds to `visitor`, once each.
    fn visit_roots(&mut self, visitor: &mut dyn RootVisitor);

    /// Called once after a collection relocated objects and every root was
    /// forwarded. For state derived from the heap's layout rather than from
    /// a pointer (a TLAB's cached allocation cursor); most holders have none.
    fn relocated(&mut self) {}
}

/// The collector's two walks over a [`RootHaver`]. Blanket-implemented, so
/// every holder walks alike: a holder supplies
/// [`visit_roots`](RootHaver::visit_roots) and
/// [`relocated`](RootHaver::relocated), and cannot change what the walks do
/// with them.
pub trait RootHaverExt: RootHaver {
    /// Record every strong root.
    fn collect_roots(&mut self, roots: &mut Vec<HeapPtr>);

    /// Repoint every root after a collection, then
    /// [`relocated`](RootHaver::relocated).
    fn forward_roots(&mut self, forward: &mut ForwardRoots<'_>);
}

impl<T: RootHaver + ?Sized> RootHaverExt for T {
    fn collect_roots(&mut self, roots: &mut Vec<HeapPtr>) {
        self.visit_roots(&mut CollectRoots(roots));
    }

    fn forward_roots(&mut self, forward: &mut ForwardRoots<'_>) {
        self.visit_roots(forward);
        self.relocated();
    }
}

impl RootHaver for () {
    fn visit_roots(&mut self, _visitor: &mut dyn RootVisitor) {}
}

impl RootHaver for HeapPtr {
    fn visit_roots(&mut self, visitor: &mut dyn RootVisitor) {
        visitor.visit(self);
    }
}

impl RootHaver for Value {
    fn visit_roots(&mut self, visitor: &mut dyn RootVisitor) {
        if let Some(ptr) = self.as_object_ptr() {
            let mut moved = ptr;
            visitor.visit(&mut moved);
            if moved != ptr {
                *self = Value::object(moved);
            }
        }
    }
}

impl RootHaver for crate::TypeHead {
    fn visit_roots(&mut self, visitor: &mut dyn RootVisitor) {
        self.visit_root(visitor);
    }
}

impl RootHaver for crate::RealizedTy {
    fn visit_roots(&mut self, visitor: &mut dyn RootVisitor) {
        self.visit_heads_mut(&mut |head: &mut crate::TypeHead| head.visit_roots(visitor));
    }
}

impl RootHaver for crate::types::TypeValue {
    fn visit_roots(&mut self, visitor: &mut dyn RootVisitor) {
        self.ty.visit_roots(visitor);
    }
}

impl<T: ?Sized + RootHaver> RootHaver for Box<T> {
    fn visit_roots(&mut self, visitor: &mut dyn RootVisitor) {
        (**self).visit_roots(visitor);
    }
}

impl<T: RootHaver> RootHaver for [T] {
    fn visit_roots(&mut self, visitor: &mut dyn RootVisitor) {
        for item in self {
            item.visit_roots(visitor);
        }
    }
}

impl<T: RootHaver> RootHaver for Vec<T> {
    fn visit_roots(&mut self, visitor: &mut dyn RootVisitor) {
        self.as_mut_slice().visit_roots(visitor);
    }
}

impl<T: RootHaver> RootHaver for Option<T> {
    fn visit_roots(&mut self, visitor: &mut dyn RootVisitor) {
        if let Some(item) = self {
            item.visit_roots(visitor);
        }
    }
}

impl<A: RootHaver, B: RootHaver> RootHaver for (A, B) {
    fn visit_roots(&mut self, visitor: &mut dyn RootVisitor) {
        self.0.visit_roots(visitor);
        self.1.visit_roots(visitor);
    }
}

/// A type-erased proof that an active heap permit is held in the current
/// scope (for at least lifetime `'a`).
///
/// `PermitProof<'a>` is a zero-sized lifetime witness. It carries no
/// runtime data — the GC-exclusion guarantee comes purely from the
/// lifetime, which is bound by the originating permit's borrow.
///
/// # Where it lives
///
/// Defined here in `bex_vm_types` (rather than `bex_heap`, where the
/// permit machinery lives) so that upstream types like
/// [`crate::SharedGlobals`] — which themselves need to gate their
/// `UnsafeCell`-backed reads on "an active permit is held" without taking
/// a dependency on `bex_heap` — can accept the proof safely.
/// `bex_heap::ActiveHeapPermit::proof` is the canonical safe constructor;
/// it re-exports this type for backward compatibility.
///
/// # Why the constructor is `unsafe`
///
/// The whole point of `PermitProof` is that *holding* one is a runtime
/// witness that a permit is active. Construction must therefore be
/// gated: only callers who genuinely hold a permit may produce one. The
/// only safe production caller is
/// `bex_heap::ActiveHeapPermit::proof`, which calls
/// [`Self::new`] inside an `unsafe` block whose safety argument is
/// "`&self` proves an active permit is held for this borrow's lifetime."
/// Test mocks may use [`Self::new`] directly with a documented safety
/// argument.
#[derive(Clone, Copy)]
pub struct PermitProof<'a> {
    _marker: PhantomData<&'a ()>,
}

impl PermitProof<'_> {
    /// Construct a `PermitProof` tied to lifetime `'a`.
    ///
    /// # Safety
    ///
    /// Caller must hold an active heap permit for at least lifetime `'a`.
    /// The only intended production caller is
    /// `bex_heap::ActiveHeapPermit::proof`, which justifies the
    /// invariant via its `&self` borrow.
    #[inline]
    #[must_use]
    #[allow(unsafe_code, reason = "construction is the unsafety boundary")]
    pub const unsafe fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}
