//! The small-object allocation policy: size-class segregation, as in Go and
//! JavaScriptCore. Everything that depends on how small cells are placed and
//! how freed ones are reused is in this module; the rest of the space (start
//! and mark bitmaps, large objects, descriptors, roots, sweeping of the dead)
//! does not depend on it.
//!
//! - Every small cell size is rounded up to one of [`SIZE_CLASSES`].
//! - A block holds cells of one class only, in fixed slots after its header.
//! - A slot is free when its start bit is clear. Each block hands its free
//!   slots out lowest address first, through a cursor over its start
//!   bitmap; nothing is written into a free slot.
//! - A sweep frees a dead cell by clearing its start bit, rewinds each
//!   block's cursor, returns blocks with no cells to a shared pool that
//!   serves every class, and lists the others with room as partial blocks
//!   of their class.
//! - Each mutator caches one current block per class ([`SmallAllocator`]).
//! - The collector copies survivors into *to-space* blocks it takes from the
//!   pool ([`SmallSpace::take_to_space`]): bump-allocated, cells of mixed
//!   sizes, in no class and on no partial list, so no mutator allocates into
//!   one. Empty, such a block returns to the pool like any other.
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
//!   [`take_to_space`](SmallSpace::take_to_space) and
//!   [`end_sweep`](SmallSpace::end_sweep), and [`stats`](SmallSpace::stats)
//!   and [`occupancy`](SmallSpace::occupancy).
//! - [`BlockMeta`]: the policy's part of each block header.

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering};

use parking_lot::Mutex;

use super::{
    BLOCK_BYTES, GRANULE_BYTES, LARGE_OBJECT_THRESHOLD,
    block::{BlockHeader, FIRST_CELL_OFFSET, FIRST_GRANULE, GRANULE_WORDS},
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

/// Hint that `address` will be written soon.
#[inline(always)]
#[allow(clippy::inline_always, unsafe_code)]
fn prefetch_write(address: usize) {
    #[cfg(target_arch = "aarch64")]
    // SAFETY: a prefetch is a hint; it never faults or changes memory.
    unsafe {
        std::arch::asm!("prfm pstl1keep, [{0}]", in(reg) address, options(nostack, preserves_flags, readonly));
    }
    #[cfg(target_arch = "x86_64")]
    // SAFETY: as above.
    unsafe {
        std::arch::x86_64::_mm_prefetch::<{ std::arch::x86_64::_MM_HINT_T0 }>(address as *const i8);
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    let _ = address;
}

/// How many slots a block of each class has.
const SLOTS: [usize; CLASSES] = {
    let mut slots = [0; CLASSES];
    let mut class = 0;
    while class < CLASSES {
        slots[class] = (BLOCK_BYTES - FIRST_CELL_OFFSET) / SIZE_CLASSES[class];
        class += 1;
    }
    slots
};

/// Per class, the start granules of its slots as a block bitmap: a slot is
/// free exactly when its bit here is set and its start bit is clear.
static SLOT_MASKS: [[u64; GRANULE_WORDS]; CLASSES] = {
    let mut masks = [[0; GRANULE_WORDS]; CLASSES];
    let mut class = 0;
    while class < CLASSES {
        let stride = SIZE_CLASSES[class] / GRANULE_BYTES;
        let mut slot = 0;
        while slot < SLOTS[class] {
            let granule = FIRST_GRANULE + slot * stride;
            masks[class][granule / 64] |= 1 << (granule % 64);
            slot += 1;
        }
        class += 1;
    }
    masks
};

/// The first bitmap word that can hold a slot.
const FIRST_SLOT_WORD: usize = FIRST_GRANULE / 64;

/// The policy's part of a block header. Written by the mutator that owns the
/// block, or by the collector while every mutator is parked.
///
/// Free slots are not threaded through the block: a slot is free when its
/// start bit is clear, so a sweep that clears dead cells' start bits has
/// freed them without writing to them. Allocation walks the start bitmap
/// with a cursor, one word at a time, lowest address first.
#[derive(Default)]
pub(crate) struct BlockMeta {
    /// A mutator allocates here; a sweep leaves the block off every list.
    owned: AtomicBool,
    /// To-space: the collector bump-allocated copies of mixed sizes here.
    /// In no class, on no partial list, with no free list; cleared when the
    /// block is taken for a class again.
    bump: AtomicBool,
    /// Held a cell when the current collection began, so its unpinned live
    /// cells are copied out. Set by [`SmallSpace::begin_collection`], cleared
    /// by [`SmallSpace::end_sweep`].
    from_space: AtomicBool,
    /// Index into [`SIZE_CLASSES`]; meaningless while the block is empty
    /// and unowned.
    class: AtomicU8,
    /// The start-bitmap word the cursor is in.
    word: AtomicUsize,
    /// The free slots of that word not yet handed out, as granule bits.
    free: AtomicU64,
}

impl BlockMeta {
    #[inline]
    fn class(&self) -> usize {
        usize::from(self.class.load(Ordering::Relaxed))
    }

    /// Start the block over as an empty block of `class`.
    fn reset(&self, header: &BlockHeader, class: usize) {
        self.bump.store(false, Ordering::Relaxed);
        self.class.store(
            u8::try_from(class).expect("fewer than 256 classes"),
            Ordering::Relaxed,
        );
        self.rewind(header);
    }

    /// Whether a mutator currently allocates into the block.
    #[inline]
    pub(crate) fn is_owned(&self) -> bool {
        self.owned.load(Ordering::Relaxed)
    }

    /// Whether the block is to-space: bump-filled by the collector.
    #[inline]
    pub(crate) fn is_bump(&self) -> bool {
        self.bump.load(Ordering::Relaxed)
    }

    /// Whether the block held a cell when the current collection began.
    #[inline]
    pub(crate) fn is_from_space(&self) -> bool {
        self.from_space.load(Ordering::Relaxed)
    }

    /// Point the cursor at the block's first free slot. Called whenever the
    /// start bitmap lost bits: on reset and after a sweep.
    fn rewind(&self, header: &BlockHeader) {
        self.word.store(FIRST_SLOT_WORD, Ordering::Relaxed);
        self.free
            .store(self.free_bits(header, FIRST_SLOT_WORD), Ordering::Relaxed);
    }

    /// The free slots of word `word` of `header`'s start bitmap.
    #[inline]
    fn free_bits(&self, header: &BlockHeader, word: usize) -> u64 {
        SLOT_MASKS[self.class()][word] & !header.starts.word(word)
    }

    /// Take a free slot of the block at `base`, lowest address first.
    ///
    /// # Safety
    /// The caller owns the block, or every mutator is parked.
    #[inline]
    unsafe fn pop(&self, base: usize) -> Option<usize> {
        let mut free = self.free.load(Ordering::Relaxed);
        if free == 0 {
            free = self.advance(base)?;
        }
        let rest = free & (free - 1);
        self.free.store(rest, Ordering::Relaxed);
        let word_base = base + self.word.load(Ordering::Relaxed) * 64 * GRANULE_BYTES;
        if rest != 0 {
            // The next cell is written soon: a reused slot is usually out of
            // cache, and its first stores would otherwise stall the caller.
            prefetch_write(word_base + rest.trailing_zeros() as usize * GRANULE_BYTES);
        }
        Some(word_base + free.trailing_zeros() as usize * GRANULE_BYTES)
    }

    /// Move the cursor to the next word with a free slot and return its
    /// free bits, or `None` when the block is full.
    #[cold]
    fn advance(&self, base: usize) -> Option<u64> {
        // SAFETY: the caller of `pop` owns the initialized block.
        let header = unsafe { BlockHeader::at(base) };
        let mut word = self.word.load(Ordering::Relaxed);
        while word + 1 < GRANULE_WORDS {
            word += 1;
            let free = self.free_bits(header, word);
            if free != 0 {
                self.word.store(word, Ordering::Relaxed);
                return Some(free);
            }
        }
        self.word.store(GRANULE_WORDS - 1, Ordering::Relaxed);
        None
    }

    /// Free slots left in the block.
    fn room(&self, header: &BlockHeader) -> usize {
        SLOTS[self.class()] - header.starts.count_ones()
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
            let header = unsafe { BlockHeader::at(base) };
            header.policy.reset(header, class);
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

    /// An empty block for the collector to copy survivors into, flagged
    /// bump-only so no mutator allocates into it and no sweep lists it.
    ///
    /// # Safety
    /// Every mutator is parked.
    pub(crate) unsafe fn take_to_space(&mut self) -> usize {
        let base = self.take_free();
        // SAFETY: pool blocks are initialized.
        let header = unsafe { BlockHeader::at(base) };
        debug_assert!(header.starts.is_empty(), "a pooled block is empty");
        header.policy.bump.store(true, Ordering::Relaxed);
        base
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
            header.policy.bump.store(false, Ordering::Relaxed);
            self.free.push(base);
        } else if !header.policy.is_bump() && header.policy.room(header) > 0 {
            self.partial[header.policy.class()].push(base);
        }
    }

    /// Collector hook, before marking: flag every block holding a cell as
    /// from-space, so a to-space block taken during the collection is told
    /// from one whose survivors are to be copied out.
    pub(crate) fn begin_collection(&self) {
        for &base in self.pool.blocks() {
            // SAFETY: pool blocks are initialized.
            let header = unsafe { BlockHeader::at(base) };
            header
                .policy
                .from_space
                .store(!header.starts.is_empty(), Ordering::Relaxed);
        }
    }

    /// Collector hook, after every dead cell is dropped and its start bit
    /// cleared: rewind every block's cursor and relist every block no
    /// mutator owns.
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
            header.policy.from_space.store(false, Ordering::Relaxed);
            header.policy.rewind(header);
            if !header.policy.is_owned() {
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
            // A pooled block is in no class; neither is a to-space block.
            if self.free.contains(&base) || header.policy.is_bump() {
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
    /// take before the allocator needs another.
    pub(crate) fn room_for(&self, bytes: usize) -> usize {
        let block = self.current[class_of(bytes)];
        if block == 0 {
            return 0;
        }
        // SAFETY: this allocator owns the block.
        let header = unsafe { BlockHeader::at(block) };
        header.policy.room(header)
    }
}
