use ::std::{collections::HashMap, marker::PhantomData};

use crate::{HeapPtr, Object, Value};

/// Where a collection moved each object: old address to new.
///
/// A collection touches this once per survivor and once per reference it
/// fixes up, so it is not a hash map: the collection registers the slot
/// ranges of the spaces it evacuates before it starts, an entry is one write
/// into a table indexed by the slot a pointer names, and a lookup is a binary
/// search over a few hundred ranges and one read. A range the collection
/// leaves in place (the old generation in a minor collection, the
/// compile-time pool) maps every pointer in it to itself, and a pointer
/// outside every range goes through a small hash map, so a map built by hand
/// with [`insert`](Self::insert) works the same way.
pub struct ForwardingMap {
    /// Sorted by address; disjoint.
    ranges: Vec<Range>,
    /// One entry per slot of the moving ranges, null until the slot's object
    /// is moved.
    slots: Vec<HeapPtr>,
    others: HashMap<HeapPtr, HeapPtr, rustc_hash::FxBuildHasher>,
    /// Entries recorded: moved objects and `others`; pointers a range maps
    /// to themselves are not counted.
    len: usize,
}

/// The slots one space occupies, as the collection sees them: moving or left
/// in place, and tagged for the collection's own use.
#[derive(Clone, Copy, Debug)]
pub struct SlotRange {
    /// Address of the first slot.
    pub start: usize,
    /// Address one past the last slot.
    pub end: usize,
    /// Whether this collection moves the range's objects. A range left in
    /// place forwards every pointer to itself.
    pub moving: bool,
    /// What the registering collection wants to know about a pointer in this
    /// range (its generation); see [`ForwardingMap::tag`].
    pub tag: u8,
}

#[derive(Clone, Copy)]
struct Range {
    start: usize,
    end: usize,
    /// Index of the range's first slot in `slots`, or `None` when the range
    /// is left in place.
    base: Option<usize>,
    tag: u8,
}

enum Located {
    InPlace,
    Slot(usize),
    Outside,
}

impl ForwardingMap {
    /// A map over `ranges`, which must be disjoint and aligned to object
    /// slots.
    pub fn with_ranges(mut ranges: Vec<SlotRange>) -> Self {
        ranges.sort_unstable_by_key(|range| range.start);
        let mut slot_count = 0;
        let ranges = ranges
            .into_iter()
            .map(|range| {
                debug_assert!(range.start <= range.end);
                let base = range.moving.then_some(slot_count);
                if range.moving {
                    slot_count += (range.end - range.start) / size_of::<Object>();
                }
                Range {
                    start: range.start,
                    end: range.end,
                    base,
                    tag: range.tag,
                }
            })
            .collect::<Vec<_>>();
        debug_assert!(
            ranges.windows(2).all(|pair| pair[0].end <= pair[1].start),
            "forwarding ranges overlap"
        );
        Self {
            ranges,
            slots: vec![HeapPtr::null(); slot_count],
            others: HashMap::default(),
            len: 0,
        }
    }

    #[inline]
    fn range(&self, ptr: HeapPtr) -> Option<&Range> {
        let addr = ptr.as_ptr() as usize;
        let index = self.ranges.partition_point(|range| range.end <= addr);
        self.ranges.get(index).filter(|range| range.start <= addr)
    }

    #[inline]
    fn locate(&self, ptr: HeapPtr) -> Located {
        match self.range(ptr) {
            Some(Range {
                base: Some(base),
                start,
                ..
            }) => Located::Slot(base + (ptr.as_ptr() as usize - start) / size_of::<Object>()),
            Some(Range { base: None, .. }) => Located::InPlace,
            None => Located::Outside,
        }
    }

    /// The tag of the range `ptr` falls in, if any.
    #[inline]
    pub fn tag(&self, ptr: HeapPtr) -> Option<u8> {
        self.range(ptr).map(|range| range.tag)
    }

    /// Where the object at `ptr` is now: `ptr` itself in a range left in
    /// place, its new address once it has been moved, nothing otherwise.
    #[inline]
    pub fn get<'a>(&'a self, ptr: &'a HeapPtr) -> Option<&'a HeapPtr> {
        match self.locate(*ptr) {
            Located::InPlace => Some(ptr),
            Located::Slot(index) => {
                let new = &self.slots[index];
                (!new.is_null()).then_some(new)
            }
            Located::Outside => self.others.get(ptr),
        }
    }

    #[inline]
    pub fn contains_key(&self, ptr: &HeapPtr) -> bool {
        self.get(ptr).is_some()
    }

    /// Record that the object at `old` is now at `new`; returns the address
    /// recorded before, if any. Recording a pointer in a range left in place
    /// is a no-op (it maps to itself already).
    pub fn insert(&mut self, old: HeapPtr, new: HeapPtr) -> Option<HeapPtr> {
        match self.locate(old) {
            Located::InPlace => {
                debug_assert!(old == new, "{old:?} is in a range left in place");
                None
            }
            Located::Slot(index) => {
                let previous = std::mem::replace(&mut self.slots[index], new);
                if previous.is_null() {
                    self.len += 1;
                    None
                } else {
                    Some(previous)
                }
            }
            Located::Outside => {
                let previous = self.others.insert(old, new);
                if previous.is_none() {
                    self.len += 1;
                }
                previous
            }
        }
    }

    /// Entries recorded with [`insert`](Self::insert).
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl Default for ForwardingMap {
    fn default() -> Self {
        Self::with_ranges(Vec::new())
    }
}

impl ForwardingMap {
    /// Where the object at `ptr` is now, which this collection must have
    /// recorded.
    ///
    /// # Panics
    ///
    /// If `ptr` was not forwarded.
    pub fn forwarded(&self, ptr: HeapPtr) -> HeapPtr {
        self.get(&ptr)
            .copied()
            .unwrap_or_else(|| panic!("{ptr:?} was not forwarded"))
    }
}

impl std::fmt::Debug for ForwardingMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ForwardingMap")
            .field("ranges", &self.ranges.len())
            .field("slots", &self.slots.len())
            .field("len", &self.len)
            .finish()
    }
}

/// A trait for types that have heap roots.
///
/// Allows the GC to find all roots and update them when the GC moves objects.
///
/// `Send` is required because instances are tracked across tokio tasks via the heap
/// permit manager. `Sync` is intentionally not required — the permit/mutex enforces
/// that at most one thread accesses the inner value at a time.
pub trait RootHaver: Send {
    /// Collect all heap roots in this object.
    fn collect_roots(&self, roots: &mut Vec<HeapPtr>);
    /// Forward the heap pointers of all roots in this object.
    fn forward_roots(&mut self, roots: &ForwardingMap);
}

impl RootHaver for () {
    fn collect_roots(&self, _roots: &mut Vec<HeapPtr>) {}
    fn forward_roots(&mut self, _roots: &ForwardingMap) {}
}

/// Generational write-barrier hook for heap mutations that touch a [`Value`].
///
/// Implemented by `bex_heap::BexHeap` (`bex_heap` is downstream of this
/// crate, so we can't name it here directly). Lives in `bex_vm_types` so
/// types like [`crate::Future`] — which perform their own
/// `UnsafeCell<MaybeUninit<Value>>` writes via `unsafe` setters — can
/// require callers to fire the barrier through this trait without taking a
/// dependency on the heap crate.
///
/// # Why this exists
///
/// The generational GC tracks cross-generation references via a card table.
/// When an older-generation object is mutated to hold a younger-generation
/// reference, the container's card must be marked dirty so a partial
/// (Minor) GC can rediscover the reference. Heap-mutation sites in the VM
/// (`vm.rs`) call `BexHeap::write_barrier` for this. The `Future` heap
/// object is special: its terminal-state writes don't go through the VM —
/// they go through the engine's spawned `run_future` writeback path. That
/// path now must fire the barrier too, and `Future::set_ready` /
/// `set_error` enforces it via this trait at compile time.
pub trait WriteBarrier {
    /// Mark a card dirty for `container` if `value` is a younger-gen
    /// `Value::Object`. Called *before* the actual field write.
    fn write_barrier(&self, container: HeapPtr, value: Value);
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

#[cfg(test)]
mod forwarding_tests {
    use super::*;

    #[allow(unsafe_code)]
    fn ptr(addr: usize) -> HeapPtr {
        // SAFETY: used only as an address; never dereferenced.
        #[cfg(not(feature = "heap_debug"))]
        unsafe {
            HeapPtr::from_ptr(addr as *mut Object)
        }
        #[cfg(feature = "heap_debug")]
        unsafe {
            HeapPtr::from_ptr(addr as *mut Object, 0)
        }
    }

    #[test]
    fn ranges_index_slots_and_in_place_ranges_forward_to_themselves() {
        let slot = size_of::<Object>();
        let mut map = ForwardingMap::with_ranges(vec![
            SlotRange {
                start: 1000 * slot,
                end: 1004 * slot,
                moving: true,
                tag: 2,
            },
            SlotRange {
                start: 10 * slot,
                end: 12 * slot,
                moving: true,
                tag: 0,
            },
            SlotRange {
                start: 500 * slot,
                end: 502 * slot,
                moving: false,
                tag: 9,
            },
        ]);
        assert!(map.is_empty());
        assert_eq!(map.get(&ptr(11 * slot)), None);
        assert_eq!(map.insert(ptr(11 * slot), ptr(7)), None);
        assert_eq!(map.insert(ptr(1003 * slot), ptr(8)), None);
        assert_eq!(map.insert(ptr(1003 * slot), ptr(9)), Some(ptr(8)));
        assert_eq!(map.get(&ptr(11 * slot)).copied(), Some(ptr(7)));
        assert_eq!(map.forwarded(ptr(1003 * slot)), ptr(9));
        assert_eq!(map.get(&ptr(10 * slot)), None, "the slot beside it");
        assert_eq!(map.get(&ptr(1002 * slot)), None);
        assert_eq!(map.len(), 2);
        assert_eq!(map.tag(ptr(11 * slot)), Some(0));
        assert_eq!(map.tag(ptr(1000 * slot)), Some(2));
        assert_eq!(map.tag(ptr(12 * slot)), None, "one past the range");
        let kept = ptr(501 * slot);
        assert!(map.contains_key(&kept));
        assert_eq!(map.get(&kept).copied(), Some(kept));
        assert_eq!(map.tag(kept), Some(9));
        assert_eq!(map.insert(kept, kept), None);
        assert_eq!(map.len(), 2, "an in-place pointer is not an entry");
        let outside = ptr(3 * slot);
        assert_eq!(map.get(&outside), None);
        assert_eq!(map.insert(outside, ptr(4)), None);
        assert_eq!(map.get(&outside).copied(), Some(ptr(4)));
        assert_eq!(map.len(), 3);
    }

    #[test]
    fn a_map_without_ranges_is_a_hash_map() {
        let mut map = ForwardingMap::default();
        assert_eq!(map.insert(ptr(16), ptr(32)), None);
        assert_eq!(map.insert(ptr(16), ptr(48)), Some(ptr(32)));
        assert_eq!(map.forwarded(ptr(16)), ptr(48));
        assert!(!map.contains_key(&ptr(32)));
        assert_eq!(map.len(), 1);
    }
}
