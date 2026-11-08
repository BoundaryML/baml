//! Single-owner, non-moving mark-region heap for baml_rt.
//!
//! Raw-pointer API: callers must publish all live roots before allocation or
//! collection, spill running registers, and keep reported stack ranges readable.
//! Descriptors and type-tag classification are immutable for the heap lifetime.
use std::{
    alloc::{Layout, alloc_zeroed, dealloc},
    cell::Cell,
    collections::{BTreeMap, HashMap},
    marker::PhantomData,
    rc::Rc,
    time::{Duration, Instant},
};

/// Refusal after a full collection/retry, or an unrepresentable request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AllocFailure {
    pub bytes: usize,
}

pub type DescId = u32;
pub type TypeId = u32;
pub type FinalizerId = u32;
pub const BLOCK_BYTES: usize = 32768;
pub const LINE_BYTES: usize = 128;
pub const LARGE_THRESHOLD: usize = 8192;
pub const MIN_BUDGET: usize = 32 * 1024 * 1024;
const SLOTS: usize = BLOCK_BYTES / 16;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub desc: DescId,
    pub aux: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TaggedSlot {
    pub tag: u64,
    pub payload: u64,
}
#[derive(Clone, Copy, Debug)]
pub enum Kind {
    /// Headerless record, referenced only by a list/map element layout.
    InlineElement,
    Class,
    List,
    Map,
    String,
    Closure,
    Future,
    Opaque,
}
#[derive(Clone, Copy, Debug)]
pub enum SlotKind {
    Scalar,
    Pointer,
    Tagged,
}
#[derive(Clone, Copy, Debug)]
pub struct Field {
    pub name: &'static str,
    pub offset: usize,
    pub slot: SlotKind,
    pub type_id: TypeId,
}
/// Inline repeated elements begin at `offset`. Count is derived from allocation
/// bytes (excluding alignment padding). Descriptor elements describe inline map
/// entries, not pointers to separately allocated entries.
#[derive(Clone, Copy, Debug)]
pub struct Elements {
    pub offset: usize,
    pub layout: ElementLayout,
}
#[derive(Clone, Copy, Debug)]
pub enum ElementLayout {
    Scalar,
    Pointer,
    Tagged,
    Descriptor(DescId),
}
#[derive(Clone, Copy, Debug)]
pub struct Descriptor {
    pub kind: Kind,
    /// Full object size for fixed objects; element stride when `elem` is set.
    pub size: usize,
    pub ptr_slots: (usize, usize),
    pub tagged_slots: (usize, usize),
    pub elem: Option<Elements>,
    pub type_id: TypeId,
    pub fields: &'static [Field],
    pub finalizer: Option<FinalizerId>,
}

pub trait RootSource {
    fn conservative_ranges(&self, visit: &mut dyn FnMut(*const usize, *const usize));
    fn precise_roots(&self, visit: &mut dyn FnMut(*mut usize));
}
/// Useful for tests and callers with no live managed pointers.
pub struct NoRoots;
impl RootSource for NoRoots {
    fn conservative_ranges(&self, _: &mut dyn FnMut(*const usize, *const usize)) {}
    fn precise_roots(&self, _: &mut dyn FnMut(*mut usize)) {}
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub collections: usize,
    pub live_bytes: usize,
    pub allocated_since_gc: usize,
    pub budget: usize,
    pub max_pause: Duration,
    pub total_pause: Duration,
    pub reserved_bytes: usize,
}
/// Collector-owned observations; counts refer to rt object allocations, not VM slots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollectionEvent {
    Started,
    Completed {
        live_slots: usize,
        reclaimed_slots: usize,
        live_payload_bytes: usize,
    },
}

/// Opaque completion token. Keep it pending until cleanup has returned (even if
/// it throws). Publish resurrected pointers as roots before completing it.
#[derive(Debug)]
pub struct Finalizer {
    object: usize,
    pub cleanup: FinalizerId,
}
impl Finalizer {
    pub fn object(&self) -> *mut u8 {
        self.object as *mut u8
    }
}
struct Finalizable {
    object: usize,
    cleanup: FinalizerId,
    state: u8,
}

struct Memory {
    ptr: *mut u8,
    layout: Layout,
}
impl Memory {
    fn try_slab(size: usize) -> Option<Self> {
        let layout = Layout::from_size_align(size, size).ok()?;
        // Do not fault in small pages before giving Linux its huge-page hint.
        // The allocator zeroes each free run before publishing any object.
        let ptr = unsafe { std::alloc::alloc(layout) };
        if ptr.is_null() {
            return None;
        }
        #[cfg(target_os = "linux")]
        unsafe {
            // Best effort only: correctness never depends on huge-page support.
            libc::madvise(ptr.cast(), size, libc::MADV_HUGEPAGE);
        }
        Some(Self { ptr, layout })
    }
    fn try_new(size: usize, align: usize) -> Option<Self> {
        let layout = Layout::from_size_align(size, align).ok()?;
        let ptr = unsafe { alloc_zeroed(layout) };
        if ptr.is_null() {
            None
        } else {
            Some(Self { ptr, layout })
        }
    }
}
impl Drop for Memory {
    fn drop(&mut self) {
        unsafe { dealloc(self.ptr, self.layout) }
    }
}
struct Block {
    base: *mut u8,
    starts: [u64; SLOTS / 64],
    marks: [u64; SLOTS / 64],
    start_words: u32,
    sizes: [u16; SLOTS],
    lines: [u8; BLOCK_BYTES / LINE_BYTES],
    needs_sweep: bool,
    next_line: usize,
}
fn bit(bits: &[u64; SLOTS / 64], i: usize) -> bool {
    bits[i / 64] & (1 << (i % 64)) != 0
}
fn set(bits: &mut [u64; SLOTS / 64], i: usize) {
    bits[i / 64] |= 1 << (i % 64);
}
impl Block {
    fn new(base: *mut u8) -> Self {
        Self {
            base,
            starts: [0; 32],
            marks: [0; 32],
            start_words: 0,
            sizes: [0; SLOTS],
            lines: [0; 256],
            needs_sweep: false,
            next_line: 0,
        }
    }
    fn sweep(&mut self) {
        if self.needs_sweep {
            self.start_words = 0;
            for i in 0..32 {
                self.starts[i] &= self.marks[i];
                if self.starts[i] != 0 {
                    self.start_words |= 1 << i;
                }
            }
            self.needs_sweep = false;
        }
    }
    fn candidate(&self, address: usize) -> Option<usize> {
        let offset = address - self.base as usize;
        if offset >= BLOCK_BYTES {
            return None;
        }
        let slot = offset / 16;
        let mut word = slot / 64;
        let mut bits = self.starts[word] & (u64::MAX >> (63 - slot % 64));
        if bits == 0 {
            // A second-level bitmap bounds predecessor lookup to two words,
            // even for an interior root thousands of bytes after its header.
            let preceding = self.start_words & ((1u32 << word) - 1);
            if preceding == 0 {
                return None;
            }
            word = 31 - preceding.leading_zeros() as usize;
            bits = self.starts[word];
        }
        let i = word * 64 + 63 - bits.leading_zeros() as usize;
        if self.needs_sweep && !bit(&self.marks, i) {
            return None;
        }
        (offset < i * 16 + self.sizes[i] as usize).then_some(i)
    }
    fn run(&mut self, bytes: usize) -> Option<(usize, usize)> {
        self.sweep();
        let mut low = self.next_line;
        while low < 256 {
            if self.lines[low] != 0 {
                low += 1;
                continue;
            }
            let mut high = low + 1;
            while high < 256 && self.lines[high] == 0 {
                high += 1;
            }
            if (high - low) * LINE_BYTES >= bytes {
                self.next_line = high;
                self.lines[low..high].fill(1);
                // Only free lines are touched; live objects mark every line they span.
                unsafe {
                    self.base
                        .add(low * LINE_BYTES)
                        .write_bytes(0, (high - low) * LINE_BYTES);
                }
                return Some((low * LINE_BYTES, high * LINE_BYTES));
            }
            low = high;
        }
        None
    }
}
// Only allocator-owned aligned block addresses are keys: no user-controlled
// input requiring SipHash. Shift out alignment, mix for hashbrown's H2 bits.
#[derive(Default)]
struct BlockHasher(u64);
impl std::hash::Hasher for BlockHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, _: &[u8]) {
        unreachable!("block address keys are usize")
    }
    fn write_usize(&mut self, key: usize) {
        self.0 = ((key >> 15) as u64).wrapping_mul(0x9e3779b97f4a7c15);
    }
}
struct Large {
    mem: Memory,
    bytes: usize,
    marked: bool,
}

/// Intentionally !Send and !Sync. No process-global heap state.
///
/// ```compile_fail
/// fn require_send<T: Send>() {}
/// require_send::<baml_rt_gc::Heap>();
/// ```
/// ```compile_fail
/// fn require_sync<T: Sync>() {}
/// require_sync::<baml_rt_gc::Heap>();
/// ```
pub struct Heap {
    descriptors: &'static [Descriptor],
    pointer_types: &'static [bool],
    // Stable metadata addresses avoid moving/copying kilobytes per block when
    // the directory grows and let the fast path cache its current block.
    #[allow(clippy::vec_box)]
    blocks: Vec<Box<Block>>,
    slabs: Vec<Memory>,
    slab_cursor: usize,
    slab_limit: usize,
    block_index: HashMap<usize, usize, std::hash::BuildHasherDefault<BlockHasher>>,
    large: BTreeMap<usize, Large>,
    active_metadata: *mut Block,
    validated_bytes: Vec<usize>,
    has_finalizers: bool,
    lookup_cache: Cell<(usize, usize)>,
    mark_stack: Vec<(usize, usize)>,
    cursor: usize,
    accounted_cursor: usize,
    budget_end: usize,
    limit: usize,
    search: usize,
    finalizable: Vec<Finalizable>,
    ready: Vec<Finalizer>,
    weak: Vec<BTreeMap<usize, usize>>,
    stats: Stats,
    collection_observer: Option<fn(CollectionEvent)>,
    stress: bool,
    stress_polls: usize,
    poll_word: Option<Rc<Cell<u8>>>,
    capacity_limit: usize,
    emergency_block: Option<usize>,
    emergency_cursor: usize,
    emergency_limit: usize,
    _owner: PhantomData<Rc<()>>,
}
impl Heap {
    pub fn new(descriptors: &'static [Descriptor], pointer_types: &'static [bool]) -> Self {
        for (id, d) in descriptors.iter().enumerate() {
            assert!((d.type_id as usize) < pointer_types.len());
            assert!(d.size > 0);
            if matches!(d.kind, Kind::InlineElement) {
                assert!(d.elem.is_none() && d.finalizer.is_none() && d.size % 8 == 0);
                assert!(descriptors.iter().any(|parent| matches!(parent.kind, Kind::List | Kind::Map)
                    && matches!(parent.elem, Some(Elements { layout: ElementLayout::Descriptor(entry), .. }) if entry as usize == id)),
                    "inline element must belong to a list/map buffer");
            }

            let bound = d.elem.map_or(d.size, |e| e.offset);
            Self::validate_ranges(d, bound);
            if let Some(e) = d.elem {
                assert!(e.offset >= 8 && e.offset % 8 == 0 && d.size % 8 == 0);
                let min = match e.layout {
                    ElementLayout::Scalar | ElementLayout::Pointer => 8,
                    ElementLayout::Tagged => 16,
                    ElementLayout::Descriptor(id) => {
                        let entry = &descriptors[id as usize];
                        assert!(matches!(d.kind, Kind::List | Kind::Map));
                        assert!(matches!(entry.kind, Kind::InlineElement));
                        assert!(entry.elem.is_none());
                        entry.size
                    }
                };
                assert!(d.size >= min);
            }
        }
        Self {
            descriptors,
            pointer_types,
            blocks: vec![],
            slabs: vec![],
            slab_cursor: 0,
            slab_limit: 0,
            block_index: HashMap::default(),
            large: BTreeMap::new(),
            active_metadata: std::ptr::null_mut(),
            validated_bytes: vec![usize::MAX; descriptors.len()],
            has_finalizers: descriptors.iter().any(|d| d.finalizer.is_some()),
            lookup_cache: Cell::new((usize::MAX, 0)),
            mark_stack: Vec::new(),
            cursor: 0,
            accounted_cursor: 0,
            budget_end: 0,
            limit: 0,
            search: 0,
            finalizable: vec![],
            ready: vec![],
            weak: vec![],
            stats: Stats {
                budget: MIN_BUDGET,
                ..Stats::default()
            },
            collection_observer: None,
            stress: stress_from_environment(),
            stress_polls: 0,
            poll_word: None,
            capacity_limit: usize::MAX,
            emergency_block: None,
            emergency_cursor: 0,
            emergency_limit: 0,
            _owner: PhantomData,
        }
    }
    /// Called synchronously before/after actual full collections, outside marking.
    pub fn set_collection_observer(&mut self, observer: Option<fn(CollectionEvent)>) {
        self.collection_observer = observer;
    }
    fn validate_ranges(d: &Descriptor, bound: usize) {
        for ((offset, count), stride) in [(d.ptr_slots, 8usize), (d.tagged_slots, 16)] {
            if count != 0 {
                assert!(
                    offset % 8 == 0 && (offset >= 8 || matches!(d.kind, Kind::InlineElement)),
                    "pointer/tagged range overlaps object header"
                );
                assert!(
                    offset
                        .checked_add(count.checked_mul(stride).unwrap())
                        .unwrap()
                        <= bound
                );
            }
        }
    }
    pub fn set_stress(&mut self, enabled: bool) {
        self.stress = enabled;
        self.stress_polls = 0;
        self.refresh_poll_word();
    }
    /// Bind the integrated runtime checkpoint word. Bit 1 belongs to this heap;
    /// the scheduler owns bit 0. The word and heap have the same owner thread.
    pub fn bind_poll_word(&mut self, word: Rc<Cell<u8>>) {
        self.poll_word = Some(word);
        self.refresh_poll_word();
    }
    fn refresh_poll_word(&self) {
        if let Some(word) = &self.poll_word {
            let due = self.stress || self.stats().allocated_since_gc >= self.stats.budget;
            word.set((word.get() & !2) | if due { 2 } else { 0 });
        }
    }
    #[cold]
    fn request_poll(&self) {
        if let Some(word) = &self.poll_word {
            word.set(word.get() | 2);
        }
    }
    pub fn stats(&self) -> Stats {
        let mut stats = self.stats;
        stats.allocated_since_gc = stats
            .allocated_since_gc
            .saturating_add(self.cursor - self.accounted_cursor);
        stats
    }
    /// Reset timing metrics without changing collection policy or heap state.
    pub fn reset_pause_metrics(&mut self) {
        self.stats.max_pause = Duration::ZERO;
        self.stats.total_pause = Duration::ZERO;
    }
    /// # Safety
    /// Roots must describe all live pointers and readable ranges/slots. The
    /// caller must initialize pointer slots with valid heap pointers or zero,
    /// preserve the header, and never access an object after it is collected.
    #[inline(always)]
    pub unsafe fn alloc(&mut self, desc: DescId, bytes: usize, roots: &dyn RootSource) -> *mut u8 {
        unsafe { self.try_alloc(desc, bytes, roots) }.expect("BAML out of memory")
    }
    /// Fallible counterpart of `alloc`; the successful bump path is identical.
    /// # Safety
    /// Same descriptor, initialization and root contract as `alloc`.
    #[inline(always)]
    pub unsafe fn try_alloc(
        &mut self,
        desc: DescId,
        bytes: usize,
        roots: &dyn RootSource,
    ) -> Result<*mut u8, AllocFailure> {
        let aligned = bytes.checked_add(15).ok_or(AllocFailure { bytes })? & !15;
        if bytes > LARGE_THRESHOLD
            || self.validated_bytes[desc as usize] != bytes
            || self.limit - self.cursor < aligned
            || self.cursor + aligned >= self.budget_end
        {
            return unsafe { self.alloc_slow(desc, bytes, aligned, roots) };
        }
        Ok(unsafe { self.bump(desc, bytes, aligned) })
    }
    /// Limit usable ordinary block capacity plus live large-object backing.
    /// Existing blocks are retained; set before ordinary allocations for a hard
    /// cap. Slab reservation and the emergency block are excluded.
    pub fn set_capacity_limit(&mut self, bytes: usize) {
        self.capacity_limit = bytes;
    }
    fn capacity_available(&self, bytes: usize) -> bool {
        let blocks = self.blocks.len() - usize::from(self.emergency_block.is_some());
        let used = self
            .large
            .values()
            .fold(blocks.saturating_mul(BLOCK_BYTES), |n, l| {
                n.saturating_add(l.mem.layout.size())
            });
        used.checked_add(bytes)
            .is_some_and(|total| total <= self.capacity_limit)
    }
    /// Reserve one collector-traced block exclusively for error construction.
    /// Must succeed before admitting BAML execution. Ordinary allocation never
    /// consumes it; collection reclaims dead reserve objects normally.
    pub fn reserve_emergency(&mut self) -> Result<(), AllocFailure> {
        if self.emergency_block.is_some() {
            return Ok(());
        }
        let mem =
            Memory::try_new(BLOCK_BYTES, BLOCK_BYTES).ok_or(AllocFailure { bytes: BLOCK_BYTES })?;
        let index = self.blocks.len();
        self.block_index.insert(mem.ptr as usize, index);
        self.blocks.push(Box::new(Block::new(mem.ptr)));
        self.slabs.push(mem);
        self.stats.reserved_bytes += BLOCK_BYTES;
        self.emergency_block = Some(index);
        Ok(())
    }
    /// Allocate from the pre-reserved block without allocating host backing or
    /// collecting. Returns refusal if retained exception objects exhaust it.
    /// # Safety
    /// Same initialization/descriptor contract as `alloc`; finalizers and large
    /// objects are not permitted in this error-only reserve.
    pub unsafe fn alloc_emergency(
        &mut self,
        desc: DescId,
        bytes: usize,
    ) -> Result<*mut u8, AllocFailure> {
        let failure = AllocFailure { bytes };
        let aligned = bytes.checked_add(15).ok_or(failure)? & !15;
        let d = &self.descriptors[desc as usize];
        assert!(
            !matches!(d.kind, Kind::InlineElement),
            "inline elements cannot be allocated"
        );
        assert!(d.finalizer.is_none() && (8..=LARGE_THRESHOLD).contains(&bytes));
        if let Some(e) = d.elem {
            assert!(bytes >= e.offset && (bytes - e.offset).is_multiple_of(d.size));
        } else {
            assert_eq!(bytes, d.size);
        }
        let index = self.emergency_block.ok_or(failure)?;
        let block = &mut self.blocks[index];
        if self.emergency_limit - self.emergency_cursor < aligned {
            let (low, high) = block.run(aligned).ok_or(failure)?;
            self.emergency_cursor = block.base as usize + low;
            self.emergency_limit = block.base as usize + high;
        }
        let p = self.emergency_cursor as *mut u8;
        let slot = (self.emergency_cursor - block.base as usize) / 16;
        self.emergency_cursor += aligned;
        set(&mut block.starts, slot);
        block.start_words |= 1 << (slot / 64);
        block.sizes[slot] = bytes as u16;
        unsafe {
            p.cast::<Header>().write(Header { desc, aux: 0 });
        }
        self.stats.allocated_since_gc = self.stats.allocated_since_gc.saturating_add(aligned);
        self.refresh_poll_word();
        Ok(p)
    }
    #[inline(always)]
    unsafe fn bump(&mut self, desc: DescId, bytes: usize, aligned: usize) -> *mut u8 {
        // Boxed metadata stays put when the block directory grows. Collection
        // invalidates the run before touching metadata; acquiring a run installs
        // a fresh pointer before the next bump.
        let b = unsafe { &mut *self.active_metadata };
        let slot = (self.cursor & (BLOCK_BYTES - 1)) / 16;
        let p = self.cursor as *mut u8;
        self.cursor += aligned;
        // Run bounds establish both indices, avoiding repeated bounds branches.
        debug_assert!(slot < SLOTS);
        unsafe {
            *b.starts.get_unchecked_mut(slot / 64) |= 1 << (slot % 64);
            b.start_words |= 1 << (slot / 64);
            *b.sizes.get_unchecked_mut(slot) = bytes as u16;
        }
        unsafe {
            p.cast::<Header>().write(Header { desc, aux: 0 });
        }
        self.register_finalizer(desc, p as usize);
        p
    }
    // Only used by the cold allocator. The fast allocation guard routes the
    // budget-crossing object here, avoiding a second branch in every bump.
    unsafe fn bump_and_request_poll(
        &mut self,
        desc: DescId,
        bytes: usize,
        aligned: usize,
    ) -> *mut u8 {
        let pointer = unsafe { self.bump(desc, bytes, aligned) };
        if self.cursor >= self.budget_end {
            self.request_poll();
        }
        pointer
    }
    #[inline(always)]
    fn register_finalizer(&mut self, desc: DescId, object: usize) {
        if self.has_finalizers
            && let Some(cleanup) = self.descriptors[desc as usize].finalizer
        {
            self.finalizable.push(Finalizable {
                object,
                cleanup,
                state: 0,
            });
        }
    }
    #[cold]
    #[inline(never)]
    unsafe fn alloc_slow(
        &mut self,
        desc: DescId,
        bytes: usize,
        aligned: usize,
        roots: &dyn RootSource,
    ) -> Result<*mut u8, AllocFailure> {
        self.settle_run();
        let d = &self.descriptors[desc as usize];
        assert!(
            !matches!(d.kind, Kind::InlineElement),
            "inline elements cannot be allocated"
        );
        assert!(bytes >= 8);
        if let Some(e) = d.elem {
            assert!(bytes >= e.offset && (bytes - e.offset).is_multiple_of(d.size));
        } else {
            assert_eq!(bytes, d.size);
        }
        self.validated_bytes[desc as usize] = bytes;
        if self.stress_allocation_due() || self.stats.allocated_since_gc >= self.stats.budget {
            unsafe {
                self.collect(roots);
            }
        }
        if bytes > LARGE_THRESHOLD {
            // 64 KiB alignment is page-aligned on supported native 4/16/64 KiB systems.
            let mem = match self
                .capacity_available(aligned)
                .then(|| Memory::try_new(aligned, 65536))
                .flatten()
            {
                Some(m) => m,
                None => {
                    unsafe {
                        self.collect(roots);
                    }
                    self.capacity_available(aligned)
                        .then(|| Memory::try_new(aligned, 65536))
                        .flatten()
                        .ok_or(AllocFailure { bytes })?
                }
            };
            let p = mem.ptr;
            unsafe {
                p.cast::<Header>().write(Header { desc, aux: 0 });
            }
            self.large.insert(
                p as usize,
                Large {
                    mem,
                    bytes,
                    marked: false,
                },
            );
            self.stats.reserved_bytes += aligned;
            self.stats.allocated_since_gc = self.stats.allocated_since_gc.saturating_add(aligned);
            self.budget_end = self.cursor.saturating_add(
                self.stats
                    .budget
                    .saturating_sub(self.stats.allocated_since_gc),
            );
            self.register_finalizer(desc, p as usize);
            self.refresh_poll_word();
            return Ok(p);
        }
        if self.limit - self.cursor >= aligned {
            return Ok(unsafe { self.bump_and_request_poll(desc, bytes, aligned) });
        }
        loop {
            while self.search < self.blocks.len() {
                let i = self.search;
                if self.emergency_block == Some(i) {
                    self.search += 1;
                    continue;
                }
                if let Some((low, high)) = self.blocks[i].run(aligned) {
                    self.active_metadata = &mut *self.blocks[i];
                    self.cursor = self.blocks[i].base as usize + low;
                    self.limit = self.blocks[i].base as usize + high;
                    self.accounted_cursor = self.cursor;
                    self.budget_end = self.cursor.saturating_add(
                        self.stats
                            .budget
                            .saturating_sub(self.stats.allocated_since_gc),
                    );
                    return Ok(unsafe { self.bump_and_request_poll(desc, bytes, aligned) });
                }
                self.search += 1;
            }
            let base = match self.fresh_block() {
                Some(m) => m,
                None => {
                    unsafe {
                        self.collect(roots);
                    }
                    // Retry every reclaimed run, including partially occupied blocks.
                    for (i, block) in self.blocks.iter_mut().enumerate() {
                        if self.emergency_block == Some(i) {
                            continue;
                        }
                        if let Some((low, high)) = block.run(aligned) {
                            self.active_metadata = &mut *self.blocks[i];
                            self.search = i;
                            self.cursor = self.blocks[i].base as usize + low;
                            self.limit = self.blocks[i].base as usize + high;
                            self.accounted_cursor = self.cursor;
                            self.budget_end = self.cursor.saturating_add(
                                self.stats
                                    .budget
                                    .saturating_sub(self.stats.allocated_since_gc),
                            );
                            return Ok(unsafe { self.bump_and_request_poll(desc, bytes, aligned) });
                        }
                    }
                    self.fresh_block().ok_or(AllocFailure { bytes })?
                }
            };
            self.block_index.insert(base as usize, self.blocks.len());
            self.blocks.push(Box::new(Block::new(base)));
        }
    }
    fn fresh_block(&mut self) -> Option<*mut u8> {
        if !self.capacity_available(BLOCK_BYTES) {
            return None;
        }
        const SLAB_BYTES: usize = 2 * 1024 * 1024;
        if self.slab_cursor == self.slab_limit {
            // Failure to reserve a batch must not fail an allocation for which
            // a single spec-sized block is still available.
            let slab = Memory::try_slab(SLAB_BYTES)
                .or_else(|| Memory::try_new(BLOCK_BYTES, BLOCK_BYTES))?;
            let bytes = slab.layout.size();
            self.slab_cursor = slab.ptr as usize;
            self.slab_limit = self.slab_cursor + bytes;
            self.slabs.push(slab);
            self.stats.reserved_bytes += bytes;
        }
        let base = self.slab_cursor as *mut u8;
        self.slab_cursor += BLOCK_BYTES;
        Some(base)
    }
    fn settle_run(&mut self) {
        self.stats.allocated_since_gc = self.stats().allocated_since_gc;
        self.accounted_cursor = self.cursor;
    }
    #[inline(always)]
    fn locate(&self, address: usize) -> Option<(usize, usize)> {
        let base = address & !(BLOCK_BYTES - 1);
        let (cached_base, cached_index) = self.lookup_cache.get();
        let i = if base == cached_base {
            cached_index
        } else {
            let &i = self.block_index.get(&base)?;
            self.lookup_cache.set((base, i));
            i
        };
        self.blocks[i].candidate(address).map(|slot| (i, slot))
    }
    /// Returns the base of an allocated object containing this address.
    pub fn object_start(&self, address: usize) -> Option<usize> {
        if let Some((i, s)) = self.locate(address) {
            return Some(self.blocks[i].base as usize + s * 16);
        }
        self.large
            .range(..=address)
            .next_back()
            .and_then(|(&base, l)| (address - base < l.bytes).then_some(base))
    }
    fn marked(&self, address: usize) -> bool {
        if let Some((i, s)) = self.locate(address) {
            return bit(&self.blocks[i].marks, s);
        }
        self.large.get(&address).is_some_and(|l| l.marked)
    }
    fn mark(&mut self, address: usize, queue: &mut Vec<(usize, usize)>) {
        if address == 0 {
            return;
        }
        if let Some((i, s)) = self.locate(address) {
            let b = &mut self.blocks[i];
            if bit(&b.marks, s) {
                return;
            }
            set(&mut b.marks, s);
            let bytes = b.sizes[s] as usize;
            b.lines[s * 16 / LINE_BYTES..(s * 16 + bytes).div_ceil(LINE_BYTES)].fill(1);
            self.stats.live_bytes += (bytes + 15) & !15;
            queue.push((b.base as usize + s * 16, bytes));
        } else if let Some((&base, l)) = self.large.range_mut(..=address).next_back()
            && address - base < l.bytes
            && !l.marked
        {
            l.marked = true;
            self.stats.live_bytes += l.mem.layout.size();
            queue.push((base, l.bytes));
        }
    }
    unsafe fn slots(
        pointer_types: &[bool],
        base: usize,
        d: &Descriptor,
        visit: &mut impl FnMut(usize),
    ) {
        for i in 0..d.ptr_slots.1 {
            visit(unsafe { *((base + d.ptr_slots.0 + i * 8) as *const usize) });
        }
        for i in 0..d.tagged_slots.1 {
            let slot = unsafe { &*((base + d.tagged_slots.0 + i * 16) as *const TaggedSlot) };
            if usize::try_from(slot.tag)
                .ok()
                .and_then(|id| pointer_types.get(id))
                .copied()
                .unwrap_or(false)
            {
                visit(slot.payload as usize);
            }
        }
    }
    unsafe fn trace_object(
        descriptors: &[Descriptor],
        pointer_types: &[bool],
        object: usize,
        bytes: usize,
        visit: &mut impl FnMut(usize),
    ) {
        let h = unsafe { &*(object as *const Header) };
        let d = &descriptors[h.desc as usize];
        unsafe {
            Self::slots(pointer_types, object, d, visit);
        }
        if let Some(e) = d.elem {
            // Scalar buffers have no edges, irrespective of their capacity.
            if matches!(e.layout, ElementLayout::Scalar) {
                return;
            }
            for offset in (e.offset..bytes).step_by(d.size) {
                let base = object + offset;
                match e.layout {
                    ElementLayout::Scalar => unreachable!(),
                    ElementLayout::Pointer => visit(unsafe { *(base as *const usize) }),
                    ElementLayout::Tagged => {
                        let s = unsafe { &*(base as *const TaggedSlot) };
                        if usize::try_from(s.tag)
                            .ok()
                            .and_then(|id| pointer_types.get(id))
                            .copied()
                            .unwrap_or(false)
                        {
                            visit(s.payload as usize);
                        }
                    }
                    ElementLayout::Descriptor(id) => unsafe {
                        Self::slots(pointer_types, base, &descriptors[id as usize], visit);
                    },
                }
            }
        }
    }
    unsafe fn trace(&self, object: usize, visit: &mut impl FnMut(usize)) {
        let bytes = if let Some((i, s)) = self.locate(object) {
            self.blocks[i].sizes[s] as usize
        } else {
            self.large[&object].bytes
        };
        unsafe {
            Self::trace_object(self.descriptors, self.pointer_types, object, bytes, visit);
        }
    }
    unsafe fn drain(&mut self, queue: &mut Vec<(usize, usize)>) {
        while let Some((object, bytes)) = queue.pop() {
            // Descriptors outlive the heap. Traversal borrows only object memory,
            // so each edge can be marked directly without an intermediate vector.
            unsafe {
                Self::trace_object(
                    self.descriptors,
                    self.pointer_types,
                    object,
                    bytes,
                    &mut |edge| self.mark(edge, queue),
                );
            }
        }
    }
    /// # Safety
    /// Same root/readability contract as `alloc`. No mutator may run during mark.
    pub unsafe fn collect(&mut self, roots: &dyn RootSource) {
        if let Some(observer) = self.collection_observer {
            observer(CollectionEvent::Started);
        }
        let start = Instant::now();
        let mut before = self.large.len();
        self.cursor = 0;
        self.accounted_cursor = 0;
        self.budget_end = 0;
        self.limit = 0;
        self.search = 0;
        self.emergency_cursor = 0;
        self.emergency_limit = 0;
        self.stats.live_bytes = 0;
        for b in &mut self.blocks {
            b.sweep();
            if self.collection_observer.is_some() {
                before += b
                    .starts
                    .iter()
                    .map(|w| w.count_ones() as usize)
                    .sum::<usize>();
            }
            b.marks.fill(0);
            b.lines.fill(0);
            b.next_line = 0;
        }
        for l in self.large.values_mut() {
            l.marked = false;
        }
        let mut queue = std::mem::take(&mut self.mark_stack);
        roots.conservative_ranges(&mut |low, high| {
            assert_eq!(low as usize % 8, 0);
            assert_eq!(high as usize % 8, 0);
            assert!(low <= high);
            let mut p = low;
            while p < high {
                self.mark(unsafe { p.read() }, &mut queue);
                p = unsafe { p.add(1) };
            }
        });
        roots.precise_roots(&mut |slot| self.mark(unsafe { slot.read() }, &mut queue));
        // Pending tasks remain roots even after the owner has taken their tokens.
        let pending: Vec<_> = self
            .finalizable
            .iter()
            .filter(|f| f.state == 1)
            .map(|f| f.object)
            .collect();
        for p in pending {
            self.mark(p, &mut queue);
        }
        unsafe {
            self.drain(&mut queue);
        }
        // Select ALL dead seeds before tracing any of them (cycles finalize together).
        let mut seeds: Vec<_> = self
            .finalizable
            .iter()
            .enumerate()
            .filter(|(_, f)| f.state == 0 && !self.marked(f.object))
            .map(|(i, _)| i)
            .collect();
        // Match the existing collector's storage-order oracle, including reused
        // slots. Large objects follow blocks, ordered by allocation address.
        seeds.sort_unstable_by_key(|&i| {
            let p = self.finalizable[i].object;
            self.locate(p)
                .map_or((1, p, 0), |(block, slot)| (0, block, slot))
        });
        for i in seeds {
            let f = &mut self.finalizable[i];
            f.state = 1;
            let p = f.object;
            self.ready.push(Finalizer {
                object: p,
                cleanup: f.cleanup,
            });
            self.mark(p, &mut queue);
        }
        unsafe {
            self.drain(&mut queue);
        }
        #[cfg(debug_assertions)]
        unsafe {
            self.verify(roots);
        }
        let mut weak = std::mem::take(&mut self.weak);
        for table in &mut weak {
            table.retain(|&key, _| self.marked(key));
        }
        self.weak = weak;
        let finals = std::mem::take(&mut self.finalizable);
        self.finalizable = finals
            .into_iter()
            .filter(|f| self.marked(f.object))
            .collect();
        for b in &mut self.blocks {
            b.needs_sweep = true;
        }
        self.large.retain(|_, l| {
            if !l.marked {
                self.stats.reserved_bytes -= l.mem.layout.size();
            }
            l.marked
        });
        self.mark_stack = queue;
        self.stress_polls = 0;
        self.stats.allocated_since_gc = 0;
        self.stats.budget = Self::budget_for(self.stats.live_bytes);
        self.stats.collections += 1;
        let pause = start.elapsed();
        self.stats.total_pause += pause;
        self.stats.max_pause = self.stats.max_pause.max(pause);
        self.refresh_poll_word();
        if let Some(observer) = self.collection_observer {
            let mut live_slots = self.large.len();
            let mut live_payload_bytes = self.large.values().map(|l| l.bytes - 8).sum::<usize>();
            for b in &self.blocks {
                for (word, &bits) in b.marks.iter().enumerate() {
                    live_slots += bits.count_ones() as usize;
                    let mut bits = bits;
                    while bits != 0 {
                        let slot = word * 64 + bits.trailing_zeros() as usize;
                        live_payload_bytes += b.sizes[slot] as usize - 8;
                        bits &= bits - 1;
                    }
                }
            }
            observer(CollectionEvent::Completed {
                live_slots,
                reclaimed_slots: before - live_slots,
                live_payload_bytes,
            });
        }
    }
    pub fn budget_for(live_bytes: usize) -> usize {
        live_bytes.saturating_mul(4).max(MIN_BUDGET)
    }
    // Tiny fixtures still collect on every cold allocation and checkpoint.
    // Rewalking a growing live graph after each of its O(n) allocations makes
    // stress quadratic. Above 256 KiB, force GC after only 1/32 of the live
    // heap has been allocated (128x the normal headroom policy), or 64 polls
    // without enough allocation. Every triggered collection is still complete.
    fn stress_allocation_due(&self) -> bool {
        self.stress
            && (self.stats.live_bytes <= 256 * 1024
                || self.stats().allocated_since_gc >= (self.stats.live_bytes / 32).max(BLOCK_BYTES))
    }
    /// # Safety
    /// Same contract as `collect`. Stress checkpoints collect eagerly on small
    /// heaps and pace full walks of large heaps by allocation pressure or polls.
    pub unsafe fn safepoint(&mut self, roots: &dyn RootSource) {
        self.stress_polls += usize::from(self.stress);
        if self.stress_allocation_due()
            || self.stress && self.stress_polls >= 64
            || self.stats().allocated_since_gc >= self.stats.budget
        {
            unsafe {
                self.collect(roots);
            }
        }
    }
    /// # Safety
    /// Root slots and traced fields must be readable, as for `collect`.
    pub unsafe fn verify(&self, roots: &dyn RootSource) {
        // Verify managed candidates, just as marking classifies them. A local
        // heap container may borrow a stack object; conservative false roots
        // can retain that container even after the borrowed frame has returned.
        // Such non-managed addresses must never be dereferenced here. Their
        // lifetime is the compiler's proof, not an object-start bitmap entry.
        let check = |p| {
            assert!(
                p == 0
                    || match self.object_start(p) {
                        Some(base) => self.marked(base),
                        None => {
                            !self.block_index.contains_key(&(p & !(BLOCK_BYTES - 1)))
                                && p % std::mem::align_of::<Header>() == 0
                        }
                    },
                "dangling precise GC pointer: {p:#x}"
            );
        };
        roots.precise_roots(&mut |slot| check(unsafe { slot.read() }));
        for b in &self.blocks {
            for (w, &bits) in b.marks.iter().enumerate() {
                let mut bits = bits;
                while bits != 0 {
                    let s = w * 64 + bits.trailing_zeros() as usize;
                    unsafe {
                        self.trace(b.base as usize + s * 16, &mut |p| check(p));
                    }
                    bits &= bits - 1;
                }
            }
        }
        for (&p, l) in &self.large {
            if l.marked {
                unsafe {
                    self.trace(p, &mut |p| check(p));
                }
            }
        }
    }
    pub fn take_pending_finalizers(&mut self) -> Vec<Finalizer> {
        std::mem::take(&mut self.ready)
    }
    pub fn complete_finalizer(&mut self, token: Finalizer) {
        let f = self
            .finalizable
            .iter_mut()
            .find(|f| f.object == token.object && f.state == 1)
            .expect("finalizer not pending on this heap");
        f.state = 2;
    }
    /// Suppress GC cleanup after explicit/defer cleanup; also valid while rooted.
    pub fn suppress_finalizer(&mut self, object: usize) {
        if let Some(f) = self.finalizable.iter_mut().find(|f| f.object == object) {
            assert_eq!(f.state, 0);
            f.state = 2;
        }
    }
    /// Weak keys with opaque non-owning scalar values (not ephemerons).
    pub fn new_weak_table(&mut self) -> usize {
        self.weak.push(BTreeMap::new());
        self.weak.len() - 1
    }
    pub fn weak_insert(&mut self, table: usize, key: usize, value: usize) {
        assert_eq!(self.object_start(key), Some(key));
        self.weak[table].insert(key, value);
    }
    pub fn weak_get(&self, table: usize, key: usize) -> Option<usize> {
        self.weak[table].get(&key).copied()
    }
}
/// write barrier. `obj` is reserved for future sticky marks.
/// # Safety
/// `slot` must be a writable, aligned pointer field of live `obj`; `value` must
/// be null or a valid object pointer, with no conflicting Rust references. A
/// stack object's storage must outlive the borrow and remain in a conservative
/// root range for as long as a heap object can reach it.
#[inline(always)]
pub unsafe fn write_ptr(_obj: *mut u8, slot: *mut usize, value: usize) {
    unsafe {
        slot.write(value);
    }
}

// Deliberately standalone: the heap has no interpreter/environment dependency.
#[allow(clippy::disallowed_methods)]
fn stress_from_environment() -> bool {
    std::env::var("BAML_GC_STRESS").is_ok_and(|v| v == "1")
}

#[cfg(test)]
mod allocation_tests {
    use super::*;
    static DESCS: [Descriptor; 2] = [
        Descriptor {
            kind: Kind::Class,
            size: 32,
            ptr_slots: (0, 0),
            tagged_slots: (0, 0),
            elem: None,
            type_id: 0,
            fields: &[],
            finalizer: None,
        },
        Descriptor {
            kind: Kind::List,
            size: 8,
            ptr_slots: (0, 0),
            tagged_slots: (0, 0),
            elem: Some(Elements {
                offset: 8,
                layout: ElementLayout::Scalar,
            }),
            type_id: 0,
            fields: &[],
            finalizer: None,
        },
    ];
    #[test]
    fn checkpoint_word_tracks_small_large_and_stress_collection() {
        let word = Rc::new(Cell::new(1)); // scheduler bit must survive heap changes
        let mut h = heap(48);
        h.bind_poll_word(word.clone());
        assert_eq!(word.get(), 1);
        unsafe {
            h.alloc(0, 32, &NoRoots);
            assert_eq!(word.get(), 1);
            h.alloc(0, 32, &NoRoots);
            assert_eq!(word.get(), 3);
            h.safepoint(&NoRoots);
            assert_eq!(word.get(), 1);
            h.stats.budget = 8192;
            h.alloc(1, 16384, &NoRoots);
            assert_eq!(word.get(), 3);
            h.safepoint(&NoRoots);
            assert_eq!(word.get(), 1);
            h.set_stress(true);
            assert_eq!(word.get(), 3);
            let collections = h.stats().collections;
            h.safepoint(&NoRoots);
            h.safepoint(&NoRoots);
            assert_eq!(h.stats().collections, collections + 2);
            assert_eq!(word.get(), 3);
            h.set_stress(false);
            assert_eq!(word.get(), 1);
        }
    }
    #[test]
    fn collection_observer_reports_actual_objects_and_payload() {
        thread_local! { static EVENTS: std::cell::RefCell<Vec<CollectionEvent>> = const { std::cell::RefCell::new(Vec::new()) }; }
        fn observe(event: CollectionEvent) {
            EVENTS.with(|e| e.borrow_mut().push(event));
        }
        struct One(usize);
        impl RootSource for One {
            fn conservative_ranges(&self, _: &mut dyn FnMut(*const usize, *const usize)) {}
            fn precise_roots(&self, visit: &mut dyn FnMut(*mut usize)) {
                visit(std::ptr::from_ref(&self.0).cast_mut());
            }
        }
        let mut h = heap(usize::MAX);
        h.set_collection_observer(Some(observe));
        unsafe {
            let kept = h.alloc(0, 32, &NoRoots);
            h.alloc(0, 32, &NoRoots);
            h.alloc(1, 65536, &NoRoots);
            h.collect(&One(kept as usize));
            h.collect(&NoRoots);
            h.collect(&NoRoots);
        }
        EVENTS.with(|e| {
            assert_eq!(
                *e.borrow(),
                vec![
                    CollectionEvent::Started,
                    CollectionEvent::Completed {
                        live_slots: 1,
                        reclaimed_slots: 2,
                        live_payload_bytes: 24
                    },
                    CollectionEvent::Started,
                    CollectionEvent::Completed {
                        live_slots: 0,
                        reclaimed_slots: 1,
                        live_payload_bytes: 0
                    },
                    CollectionEvent::Started,
                    CollectionEvent::Completed {
                        live_slots: 0,
                        reclaimed_slots: 0,
                        live_payload_bytes: 0
                    },
                ]
            )
        });
    }
    fn heap(budget: usize) -> Heap {
        let mut heap = Heap::new(&DESCS, &[true]);
        heap.set_stress(false);
        heap.stats.budget = budget;
        heap
    }
    #[test]
    fn deferred_spending_is_exact_across_layouts_and_budget_crossing() {
        unsafe {
            let mut h = heap(80);
            let a = h.alloc(0, 32, &NoRoots);
            a.add(8).write_bytes(0xa5, 24);
            assert_eq!(h.stats().allocated_since_gc, 32);
            let b = h.alloc(1, 40, &NoRoots); // 48 rounded bytes; layout cache miss.
            assert_eq!(b as usize, a as usize + 32);
            assert_eq!(*a.add(8), 0xa5); // Cache miss must not zero the active run.
            assert!(
                std::slice::from_raw_parts(b.add(8), 40)
                    .iter()
                    .all(|v| *v == 0)
            );
            assert_eq!(h.stats().allocated_since_gc, 80);
            assert_eq!(h.stats().collections, 0);
            h.alloc(0, 32, &NoRoots);
            assert_eq!(h.stats().collections, 1);
            assert_eq!(h.stats().allocated_since_gc, 32);
            // Repeated statistics reads cannot settle or double-charge spending.
            assert_eq!(h.stats().allocated_since_gc, 32);
        }
    }
    #[test]
    fn allocation_crosses_budget_before_next_collection() {
        unsafe {
            let mut h = heap(48);
            h.alloc(0, 32, &NoRoots);
            h.alloc(0, 32, &NoRoots);
            assert_eq!(h.stats().allocated_since_gc, 64);
            assert_eq!(h.stats().collections, 0);
            h.safepoint(&NoRoots);
            assert_eq!(h.stats().collections, 1);
            assert_eq!(h.stats().allocated_since_gc, 0);
        }
    }
    #[test]
    fn large_allocation_settles_and_updates_active_run_budget() {
        unsafe {
            let mut h = heap(20000);
            let a = h.alloc(0, 32, &NoRoots);
            h.alloc(1, 8200, &NoRoots);
            assert_eq!(h.stats().allocated_since_gc, 8240);
            let b = h.alloc(0, 32, &NoRoots);
            assert_eq!(b as usize, a as usize + 32);
            assert_eq!(h.stats().allocated_since_gc, 8272);
            h.alloc(1, 12008, &NoRoots); // rounds to 12016 and crosses the budget.
            assert_eq!(h.stats().allocated_since_gc, 20288);
            assert_eq!(h.stats().collections, 0);
            h.alloc(0, 32, &NoRoots);
            assert_eq!(h.stats().collections, 1);
            assert_eq!(h.stats().allocated_since_gc, 32);
        }
    }
    #[test]
    fn bulk_zeroing_preserves_live_partial_lines_and_clears_reused_bytes() {
        unsafe {
            let mut h = heap(MIN_BUDGET);
            let live = h.alloc(1, 136, &NoRoots);
            live.add(8).write_bytes(0xa5, 128);
            let garbage = h.alloc(1, 8008, &NoRoots);
            garbage.add(8).write_bytes(0xee, 8000);
            struct Root(usize);
            impl RootSource for Root {
                fn conservative_ranges(&self, v: &mut dyn FnMut(*const usize, *const usize)) {
                    let p = &self.0 as *const usize;
                    v(p, unsafe { p.add(1) });
                }
                fn precise_roots(&self, _: &mut dyn FnMut(*mut usize)) {}
            }
            let root = Root(live as usize + 135);
            h.collect(&root);
            let reused = h.alloc(1, 8008, &root);
            assert_eq!(reused as usize, live as usize + 256);
            assert!(
                std::slice::from_raw_parts(reused.add(8), 8008)
                    .iter()
                    .all(|v| *v == 0)
            );
            assert!(
                std::slice::from_raw_parts(live.add(8), 128)
                    .iter()
                    .all(|v| *v == 0xa5)
            );
        }
    }
}
