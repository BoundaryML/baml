//! Blocks and how they are filled: by bumping.
//!
//! - A block is handed out empty, exactly once, and filled front to back
//!   with cells of mixed sizes ([`Bump`]): the mutator's current block
//!   ([`Tlab`](crate::Tlab)) and the collector's to-space are the same thing.
//!   Nothing is ever allocated into a block a second time.
//! - A block with a cell in it when a collection begins is *from-space*
//!   ([`BlockSpace::begin_collection`]): its unpinned live cells are copied
//!   out and, if no pin keeps it, the block goes back to the pool empty. A
//!   block with a pin is *retained*: it keeps its pinned cells, is never
//!   allocated into, and is freed by the first later collection that finds
//!   no pin in it.
//! - The pool hands out the lowest free address first and grows by a slab
//!   when it is empty.
//!
//! No free lists and no size classes: a freed slot is never reused, only a
//! whole block is. What can be lost is the tail of a block too short for the
//! next cell (at most one cell under the large-object threshold) and the
//! blocks of the mutators' current cursors, one per allocator.

use super::{
    BLOCK_BYTES,
    block::{BlockHeader, FIRST_CELL_OFFSET},
    pool::BlockPool,
};

/// A bump cursor over one block: the next free address and the block's end.
/// Empty (nothing to bump into) until its first [`refill`](Self::refill).
#[derive(Debug, Default)]
pub(crate) struct Bump {
    cursor: usize,
    end: usize,
}

impl Bump {
    /// Place a cell of `bytes` (a granule multiple) at the cursor, or `None`
    /// when the block has no room for it.
    #[inline]
    pub(crate) fn try_alloc(&mut self, bytes: usize) -> Option<usize> {
        let cell = self.cursor;
        let next = cell + bytes;
        if next > self.end {
            return None;
        }
        self.cursor = next;
        Some(cell)
    }

    /// Drop the current block and take an empty one from `space`. The old
    /// block keeps its cells until a collection empties it.
    pub(crate) fn refill(&mut self, space: &mut BlockSpace) {
        let base = space.take();
        self.cursor = base + FIRST_CELL_OFFSET;
        self.end = base + BLOCK_BYTES;
    }

    /// Give the current block back if nothing was bumped into it, and forget
    /// it either way.
    pub(crate) fn release(&mut self, space: &mut BlockSpace) {
        if self.end != 0 && self.cursor == self.end - BLOCK_BYTES + FIRST_CELL_OFFSET {
            space.release_empty(self.end - BLOCK_BYTES);
        }
        *self = Self::default();
    }

    /// Bytes left before the cursor reaches the block's end.
    pub(crate) fn remaining(&self) -> usize {
        self.end - self.cursor
    }
}

/// Counters for the blocks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct BlockStats {
    /// Blocks carved, free ones included.
    pub(crate) blocks: usize,
    /// Blocks in the pool, empty.
    pub(crate) free_blocks: usize,
}

/// What the blocks hold, from a walk of their tables.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockOccupancy {
    /// Blocks carved, free ones included.
    pub blocks: usize,
    /// Blocks in the pool, empty.
    pub free_blocks: usize,
    /// Blocks a pin of the latest collection keeps in place: never allocated
    /// into, freed once no pin remains.
    pub retained_blocks: usize,
    /// Cells allocated in blocks and not yet reclaimed.
    pub cells: usize,
}

/// The blocks and the pool of empty ones.
#[derive(Default)]
pub(crate) struct BlockSpace {
    pool: BlockPool,
    /// Empty blocks, highest address last so a pop takes the lowest.
    free: Vec<usize>,
}

impl BlockSpace {
    /// Where the blocks are, for marking, sweeping and lookups.
    pub(crate) fn pool(&self) -> &BlockPool {
        &self.pool
    }

    /// An empty block to bump into: the lowest free one, else a new slab's
    /// first.
    pub(crate) fn take(&mut self) -> usize {
        let base = self.free.pop().unwrap_or_else(|| {
            // Popped from the end: lowest address first.
            self.free.extend(self.pool.grow().rev());
            self.free.pop().expect("a new slab adds blocks")
        });
        // SAFETY: pool blocks are initialized.
        debug_assert!(
            unsafe { BlockHeader::at(base) }.starts.is_empty(),
            "a pooled block is empty"
        );
        base
    }

    /// `base` holds no cell: back to the pool.
    fn release_empty(&mut self, base: usize) {
        // SAFETY: handed out by `take`, so initialized.
        debug_assert!(unsafe { BlockHeader::at(base) }.starts.is_empty());
        self.free.push(base);
    }

    /// Collector hook, before marking: flag every block holding a cell as
    /// from-space, so a to-space block taken during the collection is told
    /// from one whose survivors are to be copied out.
    pub(crate) fn begin_collection(&self) {
        for &base in self.pool.blocks() {
            // SAFETY: pool blocks are initialized.
            let header = unsafe { BlockHeader::at(base) };
            header.set_from_space(!header.starts.is_empty());
        }
    }

    /// Collector hook, after every from-space block was emptied or retained:
    /// relist the empty blocks.
    ///
    /// # Safety
    /// Every mutator is parked.
    pub(crate) unsafe fn end_sweep(&mut self) {
        self.free.clear();
        for &base in self.pool.blocks() {
            // SAFETY: pool blocks are initialized.
            let header = unsafe { BlockHeader::at(base) };
            header.set_from_space(false);
            if header.starts.is_empty() {
                self.free.push(base);
            }
        }
        // Pop the lowest addresses first, so allocation order follows memory.
        self.free.reverse();
    }

    pub(crate) fn stats(&self) -> BlockStats {
        BlockStats {
            blocks: self.pool.blocks().len(),
            free_blocks: self.free.len(),
        }
    }

    /// Walk every block's tables.
    pub(crate) fn occupancy(&self) -> BlockOccupancy {
        let mut occupancy = BlockOccupancy {
            blocks: self.pool.blocks().len(),
            free_blocks: self.free.len(),
            ..BlockOccupancy::default()
        };
        for &base in self.pool.blocks() {
            // SAFETY: pool blocks are initialized; only their tables are read.
            let header = unsafe { BlockHeader::at(base) };
            if header.starts.is_empty() {
                continue;
            }
            occupancy.cells += header.starts.count_ones();
            occupancy.retained_blocks += usize::from(!header.pins.is_empty());
        }
        occupancy
    }
}
