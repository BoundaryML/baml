//! The small-object allocation policy: size-class segregation, as in Go and
//! JavaScriptCore. Everything that depends on how small cells are placed and
//! how freed ones are reused is in this module; the rest of the space (start
//! and mark bitmaps, large objects, descriptors, roots, sweeping of the dead)
//! does not depend on it.
//!
//! - Every small cell size is rounded up to one of [`SIZE_CLASSES`].
//! - A block holds cells of one class only, in fixed slots after its header.
//! - Each block keeps a free list of its empty slots, threaded through them,
//!   plus a never-used tail it hands out in order.
//! - A sweep rebuilds each block's free list from the start bits it left
//!   (a dead cell's start bit is gone), returns blocks with no cells to a
//!   shared pool that serves every class, and lists the others with room as
//!   partial blocks of their class.
//! - Each mutator caches one current block per class ([`SmallAllocator`]).
//!
//! Fragmentation is bounded by construction: a freed slot is reused by the
//! next cell of its class before any new block is taken for that class, and
//! an empty block is reused by any class, so a class never holds more blocks
//! than its peak population needed. What can be lost on top of that is the
//! rounding to a class (none for cells up to 256 bytes, at most 18% above),
//! the tail of a block too short for one more slot (under 2% for classes up
//! to 2 KiB, 23% for the 8 KiB class), and partly used blocks of a class
//! whose population shrank and has not regrown.
//!
//! The seam the rest of the heap uses:
//!
//! - [`cell_size`]: the size a request is rounded to.
//! - [`SmallAllocator::try_alloc`] (no lock) and [`SmallAllocator::alloc_slow`]
//!   place a cell, granule-aligned; [`SmallAllocator::release`] gives the
//!   mutator's blocks back.
//! - [`SmallSpace`], behind the heap's mutex: the collector's hooks
//!   [`begin_collection`](SmallSpace::begin_collection),
//!   [`cell_marked`](SmallSpace::cell_marked) and
//!   [`end_sweep`](SmallSpace::end_sweep), and [`stats`](SmallSpace::stats)
//!   and [`occupancy`](SmallSpace::occupancy).
//! - [`BlockMeta`]: the policy's part of each block header.

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};

use parking_lot::Mutex;

use super::{
    BLOCK_BYTES, GRANULE_BYTES, LARGE_OBJECT_THRESHOLD,
    block::{BlockHeader, FIRST_CELL_OFFSET},
    pool::BlockPool,
};

/// Every small cell size, ascending: Go's size classes up to 8 KiB that are
/// whole granules (Go's 8- and 24-byte classes are not; no cell is under 16
/// bytes, since every cell has an 8-byte header).
pub const SIZE_CLASSES: [usize; 49] = [
    16, 32, 48, 64, 80, 96, 112, 128, 144, 160, 176, 192, 208, 224, 240, 256, 288, 320, 352, 384,
    416, 448, 480, 512, 576, 640, 704, 768, 896, 1024, 1152, 1280, 1408, 1536, 1792, 2048, 2304,
    2688, 3072, 3200, 3456, 4096, 4864, 5376, 6144, 6528, 6784, 6912, 8192,
];
const CLASSES: usize = SIZE_CLASSES.len();
const _: () = assert!(SIZE_CLASSES[CLASSES - 1] == LARGE_OBJECT_THRESHOLD);

/// The class of a cell of `granules` granules, for every granule count a
/// small cell can have.
const CLASS_BY_GRANULES: [u8; LARGE_OBJECT_THRESHOLD / GRANULE_BYTES + 1] = {
    let mut table = [0_u8; LARGE_OBJECT_THRESHOLD / GRANULE_BYTES + 1];
    let mut granules = 0;
    let mut class = 0;
    while granules < table.len() {
        while SIZE_CLASSES[class] < granules * GRANULE_BYTES {
            class += 1;
        }
        table[granules] = class as u8;
        granules += 1;
    }
    table
};

/// The class a cell of `bytes` (a granule multiple, at most the large-object
/// threshold) goes in.
#[inline]
pub(crate) fn class_of(bytes: usize) -> usize {
    usize::from(CLASS_BY_GRANULES[bytes / GRANULE_BYTES])
}

/// The size a small cell of `bytes` is rounded to.
#[inline]
pub(crate) fn cell_size(bytes: usize) -> usize {
    SIZE_CLASSES[class_of(bytes)]
}

/// The bytes the slots of a block of each class span: a table, so a bump
/// allocation does not divide.
const SLOTS_SPAN: [usize; SIZE_CLASSES.len()] = {
    let mut spans = [0; SIZE_CLASSES.len()];
    let mut class = 0;
    while class < spans.len() {
        let size = SIZE_CLASSES[class];
        spans[class] = (BLOCK_BYTES - FIRST_CELL_OFFSET) / size * size;
        class += 1;
    }
    spans
};

/// The end of the slots of a block of class `class` at `base`.
#[inline]
const fn slots_end(base: usize, class: usize) -> usize {
    base + FIRST_CELL_OFFSET + SLOTS_SPAN[class]
}

/// The policy's part of a block header. Written by the mutator that owns the
/// block, or by the collector while every mutator is parked.
#[derive(Default)]
pub(crate) struct BlockMeta {
    /// A mutator allocates here; a sweep leaves the block off every list.
    owned: AtomicBool,
    /// Index into [`SIZE_CLASSES`]; meaningless while the block is empty
    /// and unowned.
    class: AtomicU8,
    /// First free slot, each holding the address of the next; zero ends it.
    free: AtomicUsize,
    /// First slot never handed out; slots from here on are free too.
    tail: AtomicUsize,
}

impl BlockMeta {
    #[inline]
    fn class(&self) -> usize {
        usize::from(self.class.load(Ordering::Relaxed))
    }

    /// Start the block over as an empty block of `class` at `base`.
    fn reset(&self, base: usize, class: usize) {
        self.class.store(
            u8::try_from(class).expect("fewer than 256 classes"),
            Ordering::Relaxed,
        );
        self.free.store(0, Ordering::Relaxed);
        self.tail.store(base + FIRST_CELL_OFFSET, Ordering::Relaxed);
    }

    /// Take a free slot of the block at `base`.
    ///
    /// # Safety
    /// The caller owns the block, or every mutator is parked.
    #[inline]
    unsafe fn pop(&self, base: usize) -> Option<usize> {
        let free = self.free.load(Ordering::Relaxed);
        if free != 0 {
            // SAFETY: a free slot holds the next free slot's address.
            self.free
                .store(unsafe { *(free as *const usize) }, Ordering::Relaxed);
            return Some(free);
        }
        let tail = self.tail.load(Ordering::Relaxed);
        let size = SIZE_CLASSES[self.class()];
        if tail + size > slots_end(base, self.class()) {
            return None;
        }
        self.tail.store(tail + size, Ordering::Relaxed);
        Some(tail)
    }

    fn has_room(&self, base: usize) -> bool {
        self.free.load(Ordering::Relaxed) != 0
            || self.tail.load(Ordering::Relaxed) + SIZE_CLASSES[self.class()]
                <= slots_end(base, self.class())
    }

    /// Thread every slot below the tail whose start bit is clear onto the
    /// free list, lowest address first.
    ///
    /// # Safety
    /// Every mutator is parked, and dead cells have been dropped.
    unsafe fn rebuild_free_list(&self, header: &BlockHeader, base: usize) {
        let size = SIZE_CLASSES[self.class()];
        let tail = self.tail.load(Ordering::Relaxed);
        let mut free = 0;
        let mut slot = tail;
        while slot > base + FIRST_CELL_OFFSET {
            slot -= size;
            if !header.starts.get(BlockHeader::granule(base, slot)) {
                // SAFETY: a free slot's memory is the policy's.
                unsafe { (slot as *mut usize).write(free) };
                free = slot;
            }
        }
        self.free.store(free, Ordering::Relaxed);
    }
}

/// Live and capacity counts for one size class.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClassOccupancy {
    /// The class's cell size.
    pub cell_bytes: usize,
    /// Blocks of this class, owned or listed.
    pub blocks: usize,
    /// Cells those blocks can hold.
    pub capacity: usize,
    /// Cells allocated in them and not yet swept.
    pub cells: usize,
}

/// Counters for the space's share of the heap.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SmallStats {
    /// Blocks carved, free ones included.
    pub(crate) blocks: usize,
    /// Blocks in the shared pool, empty.
    pub(crate) free_blocks: usize,
    /// Bytes of blocks dedicated to a class.
    pub(crate) in_use_bytes: usize,
}

/// The blocks, the pool of empty ones and each class's partial blocks.
pub(crate) struct SmallSpace {
    pool: BlockPool,
    free: Vec<usize>,
    partial: [Vec<usize>; CLASSES],
}

impl Default for SmallSpace {
    fn default() -> Self {
        Self {
            pool: BlockPool::default(),
            free: Vec::new(),
            partial: std::array::from_fn(|_| Vec::new()),
        }
    }
}

impl SmallSpace {
    /// Where the blocks are, for marking, sweeping and lookups.
    pub(crate) fn pool(&self) -> &BlockPool {
        &self.pool
    }

    /// A block of `class` for one mutator to own: a partial one of the
    /// class, else an empty one from the pool, else a new one.
    fn take(&mut self, class: usize) -> usize {
        let base = self.partial[class].pop().unwrap_or_else(|| {
            let base = self.take_free();
            // SAFETY: pool blocks are initialized.
            unsafe { BlockHeader::at(base) }.policy.reset(base, class);
            base
        });
        // SAFETY: as above.
        unsafe { BlockHeader::at(base) }
            .policy
            .owned
            .store(true, Ordering::Relaxed);
        base
    }

    fn take_free(&mut self) -> usize {
        if let Some(base) = self.free.pop() {
            return base;
        }
        // Popped from the end: lowest address first.
        self.free.extend(self.pool.grow().rev());
        self.free.pop().expect("a new slab adds blocks")
    }

    /// A mutator gives up `base`; it is listed by the room it has.
    fn release(&mut self, base: usize) {
        // SAFETY: handed out by `take`, so initialized.
        let header = unsafe { BlockHeader::at(base) };
        header.policy.owned.store(false, Ordering::Relaxed);
        self.list(header, base);
    }

    fn list(&mut self, header: &BlockHeader, base: usize) {
        if header.starts.is_empty() {
            self.free.push(base);
        } else if header.policy.has_room(base) {
            self.partial[header.policy.class()].push(base);
        }
    }

    /// Collector hook, before marking. Size classes keep nothing per
    /// collection.
    pub(crate) fn begin_collection(&self) {}

    /// Collector hook, as marking finds a live cell. Size classes need no
    /// per-cell bookkeeping: the mark bit is enough.
    #[inline]
    pub(crate) fn cell_marked(_header: &BlockHeader, _offset: usize, _bytes: usize) {}

    /// Collector hook, after every dead cell is dropped and its start bit
    /// cleared: rebuild free lists and relist every block no mutator owns.
    ///
    /// # Safety
    /// Every mutator is parked.
    pub(crate) unsafe fn end_sweep(&mut self) {
        self.free.clear();
        self.partial.iter_mut().for_each(Vec::clear);
        for index in 0..self.pool.blocks().len() {
            let base = self.pool.blocks()[index];
            // SAFETY: pool blocks are initialized.
            let header = unsafe { BlockHeader::at(base) };
            // SAFETY: forwarded to the caller.
            unsafe { header.policy.rebuild_free_list(header, base) };
            if !header.policy.owned.load(Ordering::Relaxed) {
                self.list(header, base);
            }
        }
        // Pop the lowest addresses first, so allocation order follows memory.
        self.free.reverse();
        self.partial.iter_mut().for_each(|blocks| blocks.reverse());
    }

    pub(crate) fn stats(&self) -> SmallStats {
        let blocks = self.pool.blocks().len();
        let free_blocks = self.free.len();
        SmallStats {
            blocks,
            free_blocks,
            in_use_bytes: (blocks - free_blocks) * BLOCK_BYTES,
        }
    }

    /// Blocks, capacity and allocated cells of every class, in table order.
    pub(crate) fn occupancy(&self) -> Vec<ClassOccupancy> {
        let mut classes: Vec<ClassOccupancy> = SIZE_CLASSES
            .iter()
            .map(|&cell_bytes| ClassOccupancy {
                cell_bytes,
                ..ClassOccupancy::default()
            })
            .collect();
        for &base in self.pool.blocks() {
            // SAFETY: pool blocks are initialized.
            let header = unsafe { BlockHeader::at(base) };
            let in_pool = self.free.contains(&base);
            if in_pool {
                continue;
            }
            let class = &mut classes[header.policy.class()];
            class.blocks += 1;
            class.capacity += (BLOCK_BYTES - FIRST_CELL_OFFSET) / class.cell_bytes;
            class.cells += header.starts.count_ones();
        }
        classes
    }
}

/// One mutator's current block per class.
#[derive(Debug)]
pub(crate) struct SmallAllocator {
    /// Block address per class; zero for none.
    current: [usize; CLASSES],
}

impl Default for SmallAllocator {
    fn default() -> Self {
        Self {
            current: [0; CLASSES],
        }
    }
}

impl SmallAllocator {
    /// Place a cell of `bytes` (a granule multiple, at most the large-object
    /// threshold) in the current block of its class, granule-aligned. `None`
    /// when there is none or it is full; then call [`Self::alloc_slow`].
    #[inline]
    pub(crate) fn try_alloc(&mut self, bytes: usize) -> Option<usize> {
        let block = self.current[class_of(bytes)];
        if block == 0 {
            return None;
        }
        // SAFETY: this allocator owns the block.
        unsafe { BlockHeader::at(block).policy.pop(block) }
    }

    /// Place a cell whose class has no current block with room: give the
    /// full one back and take another.
    #[cold]
    #[inline(never)]
    pub(crate) fn alloc_slow(&mut self, space: &Mutex<SmallSpace>, bytes: usize) -> usize {
        let class = class_of(bytes);
        loop {
            if let Some(cell) = self.try_alloc(bytes) {
                return cell;
            }
            let mut space = space.lock();
            if self.current[class] != 0 {
                space.release(self.current[class]);
            }
            self.current[class] = space.take(class);
        }
    }

    /// Take a block for cells of `bytes` now.
    pub(crate) fn prepare(&mut self, space: &Mutex<SmallSpace>, bytes: usize) {
        let class = class_of(bytes);
        if self.current[class] == 0 {
            self.current[class] = space.lock().take(class);
        }
    }

    /// Give every current block back.
    pub(crate) fn release(&mut self, space: &mut SmallSpace) {
        for block in self.current.iter_mut().filter(|block| **block != 0) {
            space.release(*block);
            *block = 0;
        }
    }

    /// How many more cells of `bytes` the current block of their class can
    /// take before the allocator needs another. Walks the free list.
    pub(crate) fn room_for(&self, bytes: usize) -> usize {
        let class = class_of(bytes);
        let block = self.current[class];
        if block == 0 {
            return 0;
        }
        // SAFETY: this allocator owns the block.
        let meta = unsafe { &BlockHeader::at(block).policy };
        let size = SIZE_CLASSES[class];
        let tail = (slots_end(block, class) - meta.tail.load(Ordering::Relaxed)) / size;
        let mut listed = 0;
        let mut free = meta.free.load(Ordering::Relaxed);
        while free != 0 {
            listed += 1;
            // SAFETY: a free slot holds the next free slot's address.
            free = unsafe { *(free as *const usize) };
        }
        tail + listed
    }
}
