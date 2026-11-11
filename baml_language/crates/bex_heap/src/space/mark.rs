//! Marking and copying: from roots, reach every live cell. An unpinned
//! small cell of a from-space block is copied into to-space and every
//! pointer to it is repointed as it is visited; a pinned cell, a to-space
//! copy and a large cell are marked and traced in place.
//!
//! The forwarding address of a moved cell is its new payload address,
//! written over the old cell's header (the copy carries the real header).
//! A moved cell is told from a pinned one by the pin bit, and from a
//! to-space copy by its block's from-space flag, so the old header is read
//! only where a forwarding address was written.

use crate::descriptor::CellHeader as _;
use std::{collections::HashMap, ops::Range};

use bex_vm_types::HeapPtr;

use super::{
    RegionKind,
    block::BlockHeader,
    blocks::{BlockSpace, Bump},
    cell_bytes, cell_of,
    large::LargeSpace,
    pin_cell,
    pool::BlockPool,
    region_base, region_kind,
};
use crate::{
    descriptor::{
        DescriptorTable, HEADER_BYTES, Header, RUST_PAYLOAD_OFFSET, Trace, forward_slots,
    },
    object_trace::heap_ptr_at,
};

/// What a mark found live: cells and the bytes they occupy, copies, pinned
/// and large cells alike.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Live {
    pub(crate) cells: usize,
    pub(crate) bytes: usize,
}

/// What a conservative word resolves to; see
/// [`Marker::resolve_conservative`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Resolved {
    /// The cell whose bytes contain the word.
    pub(crate) containing: Option<usize>,
    /// The cell whose bytes end exactly at the word.
    pub(crate) ending: Option<usize>,
}

impl Resolved {
    /// The resolved cells, containing first.
    pub(crate) fn cells(self) -> impl Iterator<Item = usize> {
        [self.containing, self.ending].into_iter().flatten()
    }
}

/// The to-space block being filled: a bump cursor and the block's end.
pub(crate) struct Marker<'a> {
    blocks: &'a mut BlockSpace,
    large: &'a LargeSpace,
    descriptors: &'a DescriptorTable,
    /// Addresses of the compile-time objects: never collected, never traced.
    compile_time: Range<usize>,
    /// Cells reached and not yet traced: a pinned cell, a large cell or the
    /// to-space copy of a moved one. Never a moved cell's old copy.
    stack: Vec<usize>,
    /// The block copies are bumped into, until it is full.
    to_space: Bump,
    live: Live,
    /// Every moved pointer, old to new: how holders outside the heap follow
    /// their objects.
    moved: HashMap<HeapPtr, HeapPtr>,
    /// Every distinct pointer reached in place, for re-stamping epochs.
    #[cfg(feature = "heap_debug")]
    reached: Vec<HeapPtr>,
}

impl<'a> Marker<'a> {
    pub(crate) fn new(
        blocks: &'a mut BlockSpace,
        large: &'a LargeSpace,
        descriptors: &'a DescriptorTable,
        compile_time: Range<usize>,
    ) -> Self {
        Self {
            blocks,
            large,
            descriptors,
            compile_time,
            stack: Vec::new(),
            to_space: Bump::default(),
            live: Live::default(),
            moved: HashMap::new(),
            #[cfg(feature = "heap_debug")]
            reached: Vec::new(),
        }
    }

    pub(crate) fn live(&self) -> Live {
        self.live
    }

    pub(crate) fn pool(&self) -> &BlockPool {
        self.blocks.pool()
    }

    pub(crate) fn large(&self) -> &LargeSpace {
        self.large
    }

    fn is_compile_time(&self, ptr: HeapPtr) -> bool {
        self.compile_time.contains(&(ptr.as_ptr() as usize))
    }

    /// The cells a conservative `word` may name: the one whose bytes
    /// contain it, and the one ending exactly at it (a one-past-the-end
    /// pointer, as an iterator or slice end holds). Both are kept.
    pub(crate) fn resolve_conservative(&self, word: usize) -> Resolved {
        let Some(base) = self.pool().block_containing(word) else {
            return Resolved {
                containing: self.large.cell_containing(word),
                ending: self.large.cell_ending_at(word),
            };
        };
        // SAFETY: the pool initialized every block it holds, and every set
        // start bit names a cell with a written header.
        let header = unsafe { BlockHeader::at(base) };
        Resolved {
            // SAFETY: as above.
            containing: unsafe { header.cell_containing(base, word) },
            // SAFETY: as above.
            ending: unsafe { header.cell_ending_at(base, word) },
        }
    }

    /// Pin, mark in place and queue every cell a word of `range` points
    /// into or just past. Every range must be scanned before any precise
    /// root is forwarded: a copy decision reads the pin bits.
    ///
    /// # Safety
    /// `range` is word-aligned and readable, nothing writes it meanwhile, and
    /// nothing has been copied yet this collection.
    pub(crate) unsafe fn scan_range(&mut self, range: Range<*const usize>) {
        let mut word = range.start;
        while word < range.end {
            // SAFETY: the caller guarantees the range is readable.
            let value = unsafe { word.read() };
            for cell in self.resolve_conservative(value).cells() {
                // SAFETY: resolved from a set start bit, so allocated.
                unsafe { pin_cell(cell) };
                let ptr = heap_ptr_at(cell);
                // SAFETY: an allocated cell, and the caller parks mutators.
                let kept = unsafe { self.forward(ptr) };
                debug_assert_eq!(kept, ptr, "a pinned cell forwards to itself");
            }
            // SAFETY: stays within, or one past, the range.
            word = unsafe { word.add(1) };
        }
    }

    /// Where the object `ptr` names lives after this collection, reaching it
    /// if it was not yet reached: a compile-time object, a large cell, a
    /// pinned cell and a to-space copy stay; an unpinned small cell of a
    /// from-space block is copied the first time and found through its
    /// forwarding address after. Null stays null.
    ///
    /// # Safety
    /// No mutator runs, and `ptr` is null or names a compile-time object or
    /// an allocated cell.
    pub(crate) unsafe fn forward(&mut self, ptr: HeapPtr) -> HeapPtr {
        if ptr.is_null() {
            return ptr;
        }
        if self.is_compile_time(ptr) {
            self.reached_compile_time(ptr);
            return ptr;
        }
        let address = ptr.as_ptr() as usize;
        let cell = cell_of(address);
        let base = region_base(cell);
        // SAFETY: an allocated cell lies in a live region.
        match unsafe { region_kind(base) } {
            RegionKind::Large => {
                // SAFETY: as above.
                if unsafe { LargeSpace::mark(cell) } {
                    // SAFETY: as above.
                    self.reached_in_place(ptr, cell, unsafe { LargeSpace::cell_bytes(cell) });
                }
                ptr
            }
            // SAFETY: as above.
            RegionKind::Block => unsafe { self.forward_small(ptr, address, cell, base) },
        }
    }

    /// [`Self::forward`] for a small cell.
    ///
    /// # Safety
    /// `cell` is an allocated cell of the block at `base`, and `address` is
    /// in its first granule.
    unsafe fn forward_small(
        &mut self,
        ptr: HeapPtr,
        address: usize,
        cell: usize,
        base: usize,
    ) -> HeapPtr {
        // SAFETY: forwarded to the caller.
        let header = unsafe { BlockHeader::at(base) };
        let granule = BlockHeader::granule(base, cell);
        debug_assert!(
            header.starts.get(granule),
            "forwarding {cell:#x}, which is not an allocated cell: a pointer to a reclaimed object"
        );
        let pinned = header.pins.get(granule);
        if header.marks.get(granule) {
            if pinned || !header.is_from_space() {
                return ptr;
            }
            // SAFETY: a marked, unpinned from-space cell was copied.
            return unsafe { Self::copy_of(cell, address) };
        }
        header.marks.set(granule);
        // SAFETY: allocated and unmarked, so its header is intact.
        let bytes = unsafe { Header::at(cell) }.cell_bytes();
        if pinned {
            self.reached_in_place(ptr, cell, bytes);
            return ptr;
        }
        debug_assert!(
            header.is_from_space(),
            "an unmarked cell in a to-space block: the copy did not mark it"
        );
        let new_cell = self.alloc_to_space(bytes);
        // SAFETY: `new_cell` is `bytes` of a block no one else writes, and
        // the old cell's bytes are intact; the regions cannot overlap.
        unsafe {
            std::ptr::copy_nonoverlapping(cell as *const u8, new_cell as *mut u8, bytes);
            (cell as *mut usize).write(new_cell + HEADER_BYTES);
        }
        let new_base = region_base(new_cell);
        // SAFETY: a to-space block, initialized by the pool.
        let new_header = unsafe { BlockHeader::at(new_base) };
        let new_granule = BlockHeader::granule(new_base, new_cell);
        new_header.starts.set(new_granule);
        new_header.marks.set(new_granule);
        if header.hooked.get(granule) {
            new_header.hooked.set(new_granule);
        }
        self.live.cells += 1;
        self.live.bytes += bytes;
        self.stack.push(new_cell);
        let moved = heap_ptr_at(new_cell + (address - cell));
        self.moved.insert(ptr, moved);
        moved
    }

    /// The pointer into the copy of the moved cell `cell` that corresponds
    /// to `address`, a byte of its first granule.
    ///
    /// # Safety
    /// `cell` was copied this collection: its header word holds the copy's
    /// payload address.
    unsafe fn copy_of(cell: usize, address: usize) -> HeapPtr {
        // SAFETY: forwarded to the caller.
        let new_payload = unsafe { (cell as *const usize).read() };
        heap_ptr_at(new_payload - HEADER_BYTES + (address - cell))
    }

    /// Where the intact bytes of the object `ptr` names are now, without
    /// reaching it: its copy if it moved this collection, else itself. For
    /// reading through a dead object's pointer before the keep-alive passes
    /// decide its fate.
    ///
    /// # Safety
    /// `ptr` is null or names a compile-time object or an allocated cell.
    pub(crate) unsafe fn current_copy(&self, ptr: HeapPtr) -> HeapPtr {
        if ptr.is_null() || self.is_compile_time(ptr) {
            return ptr;
        }
        let address = ptr.as_ptr() as usize;
        let cell = cell_of(address);
        let base = region_base(cell);
        // SAFETY: an allocated cell lies in a live region.
        if unsafe { region_kind(base) } == RegionKind::Large {
            return ptr;
        }
        // SAFETY: as above.
        let header = unsafe { BlockHeader::at(base) };
        let granule = BlockHeader::granule(base, cell);
        let moved =
            header.marks.get(granule) && !header.pins.get(granule) && header.is_from_space();
        if moved {
            // SAFETY: marked, unpinned and in from-space: copied.
            unsafe { Self::copy_of(cell, address) }
        } else {
            ptr
        }
    }

    /// A cell was marked where it is: count it and queue it for tracing.
    fn reached_in_place(&mut self, ptr: HeapPtr, cell: usize, bytes: usize) {
        self.live.cells += 1;
        self.live.bytes += bytes;
        self.stack.push(cell);
        #[cfg(feature = "heap_debug")]
        self.reached.push(ptr);
        #[cfg(not(feature = "heap_debug"))]
        let _ = ptr;
    }

    /// `bytes` of to-space: the current block's bump cursor, or a fresh
    /// block from the pool.
    fn alloc_to_space(&mut self, bytes: usize) -> usize {
        if let Some(cell) = self.to_space.try_alloc(bytes) {
            return cell;
        }
        self.to_space.refill(self.blocks);
        self.to_space
            .try_alloc(bytes)
            .expect("an empty block holds any small cell")
    }

    /// Trace everything queued, forwarding each pointer slot as it is
    /// visited, until nothing is left.
    ///
    /// # Safety
    /// No mutator runs.
    pub(crate) unsafe fn drain(&mut self) {
        while let Some(cell) = self.stack.pop() {
            // SAFETY: queued cells are allocated, marked, and intact.
            unsafe { self.trace(cell) };
        }
    }

    /// A compile-time object is permanent and must not lead anywhere else:
    /// marking does not trace it.
    fn reached_compile_time(&mut self, ptr: HeapPtr) {
        #[cfg(feature = "heap_debug")]
        self.reached.push(ptr);
        #[cfg(debug_assertions)]
        {
            // SAFETY: compile-time objects live for the heap's lifetime.
            let object = unsafe { ptr.get() };
            let mut edges = Vec::new();
            crate::object_trace::trace_object(object, &mut edges);
            debug_assert!(
                edges
                    .iter()
                    .all(|&edge| edge.is_null() || self.is_compile_time(edge)),
                "compile-time object {ptr:?} contains a runtime HeapPtr; marking does not \
                 trace compile-time objects and would miss it"
            );
        }
        #[cfg(not(any(debug_assertions, feature = "heap_debug")))]
        let _ = ptr;
    }

    /// Repoint every pointer `cell` holds to where its object lives now,
    /// reaching each.
    ///
    /// # Safety
    /// `cell` is an allocated, intact cell and no mutator runs.
    unsafe fn trace(&mut self, cell: usize) {
        let descriptors: &'a DescriptorTable = self.descriptors;
        // SAFETY: intact, so its header is written.
        let descriptor = descriptors.get(unsafe { Header::at(cell) }.descriptor);
        match descriptor.trace {
            // SAFETY: a Rust value sits right after the header.
            Trace::Rust(hooks) => unsafe {
                (hooks.forward)((cell + RUST_PAYLOAD_OFFSET) as *mut u8, &mut |ptr| {
                    self.forward(ptr)
                });
            },
            // SAFETY: laid out by `descriptor`.
            Trace::Slots => unsafe {
                forward_slots(descriptor, cell, cell_bytes(cell), &mut |ptr| {
                    self.forward(ptr)
                });
            },
        }
    }

    /// Whether `ptr` names an object this mark found live. Only between
    /// marking and reclaiming: afterwards a dead cell's memory may be gone.
    ///
    /// # Safety
    /// `ptr` names a compile-time object or an allocated cell.
    pub(crate) unsafe fn is_marked(&self, ptr: HeapPtr) -> bool {
        if self.is_compile_time(ptr) {
            return true;
        }
        // SAFETY: forwarded to the caller.
        unsafe { is_cell_marked(cell_of(ptr.as_ptr() as usize)) }
    }

    /// Where the object `ptr` names lives now, if the mark found it live:
    /// for a weak entry, which keeps nothing alive.
    ///
    /// # Safety
    /// As for [`Self::is_marked`].
    pub(crate) unsafe fn forwarded(&mut self, ptr: HeapPtr) -> Option<HeapPtr> {
        // SAFETY: forwarded to the caller.
        unsafe { self.is_marked(ptr) }.then(|| {
            // SAFETY: marked, so forwarding reaches nothing new.
            unsafe { self.forward(ptr) }
        })
    }

    /// Every moved pointer, old to new.
    pub(crate) fn take_moved(&mut self) -> HashMap<HeapPtr, HeapPtr> {
        std::mem::take(&mut self.moved)
    }

    /// Every distinct pointer reached in place, compile-time ones included.
    #[cfg(feature = "heap_debug")]
    pub(crate) fn take_reached(&mut self) -> Vec<HeapPtr> {
        std::mem::take(&mut self.reached)
    }
}

/// Whether the allocated cell `cell` carries a mark.
///
/// # Safety
/// `cell` is allocated.
pub(crate) unsafe fn is_cell_marked(cell: usize) -> bool {
    let base = region_base(cell);
    // SAFETY: forwarded to the caller.
    match unsafe { region_kind(base) } {
        // SAFETY: as above.
        RegionKind::Block => unsafe { BlockHeader::at(base) }
            .marks
            .get(BlockHeader::granule(base, cell)),
        // SAFETY: as above.
        RegionKind::Large => unsafe { LargeSpace::is_marked(cell) },
    }
}
