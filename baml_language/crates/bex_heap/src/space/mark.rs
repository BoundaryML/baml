//! Marking: from roots, set the side-table mark bit of every reachable cell.

use crate::descriptor::CellHeader as _;
use std::ops::Range;

use bex_vm_types::HeapPtr;

use super::{
    RegionKind, block::BlockHeader, cell_bytes, cell_of, large::LargeSpace, pool::BlockPool,
    region_base, region_kind, small::SmallSpace,
};
use crate::descriptor::{DescriptorTable, Header, RUST_PAYLOAD_OFFSET, Trace, trace_slots};

/// What a mark found live: cells and the bytes they occupy.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Live {
    pub(crate) cells: usize,
    pub(crate) bytes: usize,
}

pub(crate) struct Marker<'a> {
    pool: &'a BlockPool,
    large: &'a LargeSpace,
    descriptors: &'a DescriptorTable,
    /// Addresses of the compile-time objects: never collected, never traced.
    compile_time: Range<usize>,
    stack: Vec<HeapPtr>,
    live: Live,
    /// Every distinct pointer the mark reached, for re-stamping epochs.
    #[cfg(feature = "heap_debug")]
    reached: Vec<HeapPtr>,
}

impl<'a> Marker<'a> {
    pub(crate) fn new(
        pool: &'a BlockPool,
        large: &'a LargeSpace,
        descriptors: &'a DescriptorTable,
        compile_time: Range<usize>,
    ) -> Self {
        Self {
            pool,
            large,
            descriptors,
            compile_time,
            stack: Vec::new(),
            live: Live::default(),
            #[cfg(feature = "heap_debug")]
            reached: Vec::new(),
        }
    }

    pub(crate) fn live(&self) -> Live {
        self.live
    }

    /// Queue a precise root or an edge.
    #[inline]
    pub(crate) fn push(&mut self, ptr: HeapPtr) {
        self.stack.push(ptr);
    }

    fn is_compile_time(&self, ptr: HeapPtr) -> bool {
        self.compile_time.contains(&(ptr.as_ptr() as usize))
    }

    /// The cell whose bytes contain `word`, if `word` points into an
    /// allocated cell at all.
    pub(crate) fn resolve_conservative(&self, word: usize) -> Option<usize> {
        let Some(base) = self.pool.block_containing(word) else {
            return self.large.cell_containing(word);
        };
        // SAFETY: the pool initialized every block it holds, and every set
        // start bit names a cell with a written header.
        unsafe { BlockHeader::at(base).cell_containing(base, word) }
    }

    /// Queue every cell a word of `range` points into.
    ///
    /// # Safety
    /// `range` is word-aligned and readable, and nothing writes it meanwhile.
    pub(crate) unsafe fn scan_range(&mut self, range: Range<*const usize>) {
        let mut word = range.start;
        while word < range.end {
            // SAFETY: the caller guarantees the range is readable.
            let value = unsafe { word.read() };
            if let Some(cell) = self.resolve_conservative(value) {
                self.push(crate::object_trace::heap_ptr_at(cell));
            }
            // SAFETY: stays within, or one past, the range.
            word = unsafe { word.add(1) };
        }
    }

    /// Mark everything reachable from the queue.
    ///
    /// # Safety
    /// No mutator runs, and every queued pointer names a compile-time object
    /// or the first granule of an allocated cell.
    pub(crate) unsafe fn drain(&mut self) {
        while let Some(ptr) = self.stack.pop() {
            if ptr.is_null() {
                continue;
            }
            if self.is_compile_time(ptr) {
                self.reached_compile_time(ptr);
                continue;
            }
            let cell = cell_of(ptr.as_ptr() as usize);
            // SAFETY: the caller guarantees `cell` is allocated.
            if unsafe { self.mark(cell) } {
                #[cfg(feature = "heap_debug")]
                self.reached.push(ptr);
                // SAFETY: just marked, so allocated; mutators are parked.
                unsafe { self.trace(cell) };
            }
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

    /// Set `cell`'s mark; whether it was unmarked.
    ///
    /// # Safety
    /// `cell` is allocated.
    unsafe fn mark(&mut self, cell: usize) -> bool {
        let base = region_base(cell);
        // SAFETY: an allocated cell lies in a live region.
        let bytes = match unsafe { region_kind(base) } {
            // SAFETY: as above.
            RegionKind::Block => match unsafe { Self::mark_small(base, cell) } {
                Some(bytes) => bytes,
                None => return false,
            },
            // SAFETY: as above.
            RegionKind::Large if unsafe { LargeSpace::mark(cell) } => unsafe {
                LargeSpace::cell_bytes(cell)
            },
            RegionKind::Large => return false,
        };
        self.live.cells += 1;
        self.live.bytes += bytes;
        true
    }

    /// Mark a small cell, telling the policy. Returns its size, or
    /// `None` if it was already marked.
    ///
    /// # Safety
    /// `cell` is an allocated cell of the block at `base`.
    unsafe fn mark_small(base: usize, cell: usize) -> Option<usize> {
        // SAFETY: forwarded to the caller.
        let header = unsafe { BlockHeader::at(base) };
        let granule = BlockHeader::granule(base, cell);
        debug_assert!(
            header.starts.get(granule),
            "marking {cell:#x}, which is not an allocated cell: a pointer to a reclaimed object"
        );
        if header.marks.get(granule) {
            return None;
        }
        header.marks.set(granule);
        // SAFETY: allocated, so its header is written.
        let bytes = unsafe { Header::at(cell) }.cell_bytes();
        SmallSpace::cell_marked(header, cell - base, bytes);
        Some(bytes)
    }

    /// Queue every pointer `cell` holds.
    ///
    /// # Safety
    /// `cell` is allocated and no mutator runs.
    unsafe fn trace(&mut self, cell: usize) {
        // SAFETY: allocated, so its header is written.
        let descriptor = self.descriptors.get(unsafe { Header::at(cell) }.descriptor);
        match descriptor.trace {
            // SAFETY: a Rust value sits right after the header.
            Trace::Rust(hooks) => unsafe {
                (hooks.trace)((cell + RUST_PAYLOAD_OFFSET) as *const u8, &mut self.stack);
            },
            // SAFETY: laid out by `descriptor`.
            Trace::Slots => unsafe {
                trace_slots(descriptor, cell, cell_bytes(cell), &mut self.stack)
            },
        }
    }

    /// Whether `ptr` names an object this mark found live. Only between
    /// marking and sweeping: afterwards a dead cell's memory may be gone.
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

    /// Every distinct pointer reached, compile-time ones included.
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
