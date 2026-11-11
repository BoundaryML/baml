//! Objects above the large-object threshold, each in an allocation of its
//! own, swept individually.
//!
//! An allocation is block-aligned and begins with a small prefix whose first
//! byte has the same meaning as a block header's, so masking a pointer to a
//! large cell finds its prefix and mark bit just as for a block.

use std::{
    alloc::{Layout, alloc, dealloc},
    collections::BTreeSet,
    sync::atomic::{AtomicBool, Ordering},
};

use super::{BLOCK_BYTES, GRANULE_BYTES, RegionKind, region_base};

#[repr(C)]
struct LargePrefix {
    kind: RegionKind,
    marked: AtomicBool,
    /// A conservative root named the cell during the latest collection.
    pinned: AtomicBool,
    cell_bytes: usize,
}

/// Bytes before the cell, keeping it granule-aligned.
const PREFIX_BYTES: usize = GRANULE_BYTES;
const _: () = assert!(size_of::<LargePrefix>() <= PREFIX_BYTES);

#[derive(Default)]
pub(crate) struct LargeSpace {
    /// Cell addresses.
    cells: BTreeSet<usize>,
    bytes: usize,
}

/// The prefix of the large cell `cell`.
///
/// # Safety
/// `cell` is a live large cell.
unsafe fn prefix<'a>(cell: usize) -> &'a LargePrefix {
    // SAFETY: the caller guarantees the allocation is live.
    unsafe { &*(region_base(cell) as *const LargePrefix) }
}

fn layout_for(cell_bytes: usize) -> Layout {
    Layout::from_size_align(PREFIX_BYTES + cell_bytes, BLOCK_BYTES)
        .expect("large object size overflows")
}

impl LargeSpace {
    /// Allocate a zeroed cell of `cell_bytes`.
    pub(crate) fn alloc(&mut self, cell_bytes: usize) -> usize {
        let layout = layout_for(cell_bytes);
        // SAFETY: the layout has nonzero size.
        let base = unsafe { alloc(layout) } as usize;
        if base == 0 {
            std::alloc::handle_alloc_error(layout);
        }
        // SAFETY: fresh allocation of `layout`.
        unsafe {
            (base as *mut LargePrefix).write(LargePrefix {
                kind: RegionKind::Large,
                marked: AtomicBool::new(false),
                pinned: AtomicBool::new(false),
                cell_bytes,
            });
            ((base + PREFIX_BYTES) as *mut u8).write_bytes(0, cell_bytes);
        }
        let cell = base + PREFIX_BYTES;
        self.cells.insert(cell);
        self.bytes += layout.size();
        cell
    }

    /// The large cell whose bytes contain `address`, if any.
    pub(crate) fn cell_containing(&self, address: usize) -> Option<usize> {
        let &cell = self.cells.range(..=address).next_back()?;
        // SAFETY: every member is a live cell.
        (address < cell + unsafe { Self::cell_bytes(cell) }).then_some(cell)
    }

    /// The large cell whose bytes end exactly at `address`, if any. Only
    /// the nearest cell at or below `address` can: allocations never overlap.
    pub(crate) fn cell_ending_at(&self, address: usize) -> Option<usize> {
        let &cell = self.cells.range(..address).next_back()?;
        // SAFETY: every member is a live cell.
        (address == cell + unsafe { Self::cell_bytes(cell) }).then_some(cell)
    }

    pub(crate) fn contains_cell(&self, cell: usize) -> bool {
        self.cells.contains(&cell)
    }

    /// The cell's size, rounded to granules.
    ///
    /// # Safety
    /// `cell` is a live large cell.
    pub(crate) unsafe fn cell_bytes(cell: usize) -> usize {
        // SAFETY: forwarded to the caller.
        unsafe { prefix(cell) }.cell_bytes
    }

    /// Mark `cell`; whether it was unmarked before.
    ///
    /// # Safety
    /// `cell` is a live large cell.
    pub(crate) unsafe fn mark(cell: usize) -> bool {
        // SAFETY: forwarded to the caller.
        !unsafe { prefix(cell) }.marked.swap(true, Ordering::Relaxed)
    }

    /// # Safety
    /// `cell` is a live large cell.
    pub(crate) unsafe fn is_marked(cell: usize) -> bool {
        // SAFETY: forwarded to the caller.
        unsafe { prefix(cell) }.marked.load(Ordering::Relaxed)
    }

    pub(crate) fn clear_marks(&self) {
        for &cell in &self.cells {
            // SAFETY: every member is a live cell.
            unsafe { prefix(cell) }
                .marked
                .store(false, Ordering::Relaxed);
        }
    }

    /// Pin `cell`: a conservative root names it.
    ///
    /// # Safety
    /// `cell` is a live large cell.
    pub(crate) unsafe fn pin(cell: usize) {
        // SAFETY: forwarded to the caller.
        unsafe { prefix(cell) }.pinned.store(true, Ordering::Relaxed);
    }

    /// # Safety
    /// `cell` is a live large cell.
    #[cfg_attr(not(test), allow(dead_code, reason = "read once the collector moves cells"))]
    pub(crate) unsafe fn is_pinned(cell: usize) -> bool {
        // SAFETY: forwarded to the caller.
        unsafe { prefix(cell) }.pinned.load(Ordering::Relaxed)
    }

    pub(crate) fn clear_pins(&self) {
        for &cell in &self.cells {
            // SAFETY: every member is a live cell.
            unsafe { prefix(cell) }
                .pinned
                .store(false, Ordering::Relaxed);
        }
    }

    /// Every cell, by address.
    pub(crate) fn cells(&self) -> impl Iterator<Item = usize> + '_ {
        self.cells.iter().copied()
    }

    /// Free every unmarked cell, after `drop_cell` has dropped its contents.
    /// Returns how many were freed.
    pub(crate) fn sweep(&mut self, mut drop_cell: impl FnMut(usize)) -> usize {
        let dead: Vec<usize> = self
            .cells()
            // SAFETY: every member is a live cell.
            .filter(|&cell| !unsafe { Self::is_marked(cell) })
            .collect();
        for &cell in &dead {
            drop_cell(cell);
            self.free(cell);
        }
        dead.len()
    }

    fn free(&mut self, cell: usize) {
        assert!(self.cells.remove(&cell), "freeing an unknown large cell");
        // SAFETY: a member until now, so live.
        let layout = layout_for(unsafe { Self::cell_bytes(cell) });
        self.bytes -= layout.size();
        // SAFETY: allocated in `alloc` with this layout.
        unsafe { dealloc(region_base(cell) as *mut u8, layout) };
    }

    pub(crate) fn len(&self) -> usize {
        self.cells.len()
    }

    /// Bytes held by large allocations, prefixes included.
    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }
}

impl Drop for LargeSpace {
    fn drop(&mut self) {
        let cells: Vec<usize> = self.cells().collect();
        for cell in cells {
            self.free(cell);
        }
    }
}
