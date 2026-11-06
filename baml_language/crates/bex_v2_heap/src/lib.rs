//! Single-owner, non-moving mark-region heap for runtime v2.
//!
//! Raw-pointer API: callers must publish all live roots before allocation or
//! collection, spill running registers, and keep reported stack ranges readable.
//! Descriptors and type-tag classification are immutable for the heap lifetime.
use std::{
    alloc::{Layout, alloc_zeroed, dealloc},
    collections::{BTreeMap, HashMap},
    marker::PhantomData,
    rc::Rc,
    time::{Duration, Instant},
};

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
    fn try_new(size: usize, align: usize) -> Option<Self> {
        let layout = Layout::from_size_align(size, align).expect("BAML out of memory");
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
    mem: Memory,
    starts: [u64; SLOTS / 64],
    marks: [u64; SLOTS / 64],
    sizes: [u16; SLOTS],
    lines: [u8; BLOCK_BYTES / LINE_BYTES],
    needs_sweep: bool,
}
fn bit(bits: &[u64; SLOTS / 64], i: usize) -> bool {
    bits[i / 64] & (1 << (i % 64)) != 0
}
fn set(bits: &mut [u64; SLOTS / 64], i: usize) {
    bits[i / 64] |= 1 << (i % 64);
}
impl Block {
    fn new(mem: Memory) -> Self {
        Self {
            mem,
            starts: [0; 32],
            marks: [0; 32],
            sizes: [0; SLOTS],
            lines: [0; 256],
            needs_sweep: false,
        }
    }
    fn sweep(&mut self) {
        if self.needs_sweep {
            for i in 0..32 {
                self.starts[i] &= self.marks[i];
            }
            self.needs_sweep = false;
        }
    }
    fn candidate(&self, address: usize) -> Option<usize> {
        let offset = address - self.mem.ptr as usize;
        if offset >= BLOCK_BYTES {
            return None;
        }
        let slot = offset / 16;
        let mut word = slot / 64;
        let mut bits = self.starts[word] & (u64::MAX >> (63 - slot % 64));
        loop {
            if bits != 0 {
                let i = word * 64 + 63 - bits.leading_zeros() as usize;
                if self.needs_sweep && !bit(&self.marks, i) {
                    return None;
                }
                return (offset < i * 16 + self.sizes[i] as usize).then_some(i);
            }
            if word == 0 {
                return None;
            }
            word -= 1;
            bits = self.starts[word];
        }
    }
    fn run(&mut self, bytes: usize) -> Option<(usize, usize)> {
        self.sweep();
        let mut low = 0;
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
                self.lines[low..high].fill(1);
                return Some((low * LINE_BYTES, high * LINE_BYTES));
            }
            low = high;
        }
        None
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
/// require_send::<bex_v2_heap::Heap>();
/// ```
/// ```compile_fail
/// fn require_sync<T: Sync>() {}
/// require_sync::<bex_v2_heap::Heap>();
/// ```
pub struct Heap {
    descriptors: &'static [Descriptor],
    pointer_types: &'static [bool],
    blocks: Vec<Block>,
    block_index: HashMap<usize, usize>,
    large: BTreeMap<usize, Large>,
    active: usize,
    cursor: usize,
    limit: usize,
    search: usize,
    finalizable: Vec<Finalizable>,
    ready: Vec<Finalizer>,
    weak: Vec<BTreeMap<usize, usize>>,
    stats: Stats,
    stress: bool,
    _owner: PhantomData<Rc<()>>,
}
impl Heap {
    pub fn new(descriptors: &'static [Descriptor], pointer_types: &'static [bool]) -> Self {
        for d in descriptors {
            assert!((d.type_id as usize) < pointer_types.len());
            assert!(d.size > 0);
            let bound = d.elem.map_or(d.size, |e| e.offset);
            Self::validate_ranges(d, bound);
            if let Some(e) = d.elem {
                assert!(e.offset >= 8 && e.offset % 8 == 0 && d.size % 8 == 0);
                let min = match e.layout {
                    ElementLayout::Scalar | ElementLayout::Pointer => 8,
                    ElementLayout::Tagged => 16,
                    ElementLayout::Descriptor(id) => {
                        let entry = &descriptors[id as usize];
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
            block_index: HashMap::new(),
            large: BTreeMap::new(),
            active: 0,
            cursor: 0,
            limit: 0,
            search: 0,
            finalizable: vec![],
            ready: vec![],
            weak: vec![],
            stats: Stats {
                budget: MIN_BUDGET,
                ..Stats::default()
            },
            stress: stress_from_environment(),
            _owner: PhantomData,
        }
    }
    fn validate_ranges(d: &Descriptor, bound: usize) {
        for ((offset, count), stride) in [(d.ptr_slots, 8usize), (d.tagged_slots, 16)] {
            if count != 0 {
                assert!(offset >= 8 && offset % 8 == 0);
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
    }
    pub fn stats(&self) -> Stats {
        self.stats
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
        let d = &self.descriptors[desc as usize];
        assert!(bytes >= 8);
        if let Some(e) = d.elem {
            assert!(bytes >= e.offset && (bytes - e.offset).is_multiple_of(d.size));
        } else {
            assert_eq!(bytes, d.size);
        }
        let aligned = bytes.checked_add(15).expect("BAML out of memory") & !15;
        if bytes > LARGE_THRESHOLD
            || self.limit - self.cursor < aligned
            || self.stats.allocated_since_gc >= self.stats.budget
        {
            return unsafe { self.alloc_slow(desc, bytes, aligned, roots) };
        }
        unsafe { self.bump(desc, bytes, aligned) }
    }
    #[inline(always)]
    unsafe fn bump(&mut self, desc: DescId, bytes: usize, aligned: usize) -> *mut u8 {
        let b = &mut self.blocks[self.active];
        let slot = self.cursor / 16;
        let p = unsafe { b.mem.ptr.add(self.cursor) };
        self.cursor += aligned;
        set(&mut b.starts, slot);
        b.sizes[slot] = bytes as u16;
        unsafe {
            p.write_bytes(0, aligned);
            p.cast::<Header>().write(Header { desc, aux: 0 });
        }
        self.account(desc, p as usize, aligned);
        p
    }
    #[inline(always)]
    fn account(&mut self, desc: DescId, object: usize, bytes: usize) {
        self.stats.allocated_since_gc = self.stats.allocated_since_gc.saturating_add(bytes);
        if let Some(cleanup) = self.descriptors[desc as usize].finalizer {
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
    ) -> *mut u8 {
        if self.stress || self.stats.allocated_since_gc >= self.stats.budget {
            unsafe {
                self.collect(roots);
            }
        }
        if bytes > LARGE_THRESHOLD {
            // 64 KiB alignment is page-aligned on supported native 4/16/64 KiB systems.
            let mem = match Memory::try_new(aligned, 65536) {
                Some(m) => m,
                None => {
                    unsafe {
                        self.collect(roots);
                    }
                    Memory::try_new(aligned, 65536).expect("BAML out of memory")
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
            self.account(desc, p as usize, aligned);
            return p;
        }
        loop {
            while self.search < self.blocks.len() {
                let i = self.search;
                if let Some((low, high)) = self.blocks[i].run(aligned) {
                    self.active = i;
                    self.cursor = low;
                    self.limit = high;
                    return unsafe { self.bump(desc, bytes, aligned) };
                }
                self.search += 1;
            }
            let mem = match Memory::try_new(BLOCK_BYTES, BLOCK_BYTES) {
                Some(m) => m,
                None => {
                    unsafe {
                        self.collect(roots);
                    }
                    // Retry every reclaimed run, including partially occupied blocks.
                    for (i, block) in self.blocks.iter_mut().enumerate() {
                        if let Some((low, high)) = block.run(aligned) {
                            self.active = i;
                            self.search = i;
                            self.cursor = low;
                            self.limit = high;
                            return unsafe { self.bump(desc, bytes, aligned) };
                        }
                    }
                    Memory::try_new(BLOCK_BYTES, BLOCK_BYTES).expect("BAML out of memory")
                }
            };
            self.block_index.insert(mem.ptr as usize, self.blocks.len());
            self.blocks.push(Block::new(mem));
            self.stats.reserved_bytes += BLOCK_BYTES;
        }
    }
    fn locate(&self, address: usize) -> Option<(usize, usize)> {
        if let Some(&i) = self.block_index.get(&(address & !(BLOCK_BYTES - 1))) {
            return self.blocks[i].candidate(address).map(|slot| (i, slot));
        }
        None
    }
    /// Returns the base of an allocated object containing this address.
    pub fn object_start(&self, address: usize) -> Option<usize> {
        if let Some((i, s)) = self.locate(address) {
            return Some(self.blocks[i].mem.ptr as usize + s * 16);
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
    fn mark(&mut self, address: usize, queue: &mut Vec<usize>) {
        if let Some((i, s)) = self.locate(address) {
            let b = &mut self.blocks[i];
            if bit(&b.marks, s) {
                return;
            }
            set(&mut b.marks, s);
            let bytes = b.sizes[s] as usize;
            b.lines[s * 16 / LINE_BYTES..(s * 16 + bytes).div_ceil(LINE_BYTES)].fill(1);
            self.stats.live_bytes += (bytes + 15) & !15;
            queue.push(b.mem.ptr as usize + s * 16);
        } else if let Some((&base, l)) = self.large.range_mut(..=address).next_back()
            && address - base < l.bytes
            && !l.marked
        {
            l.marked = true;
            self.stats.live_bytes += l.mem.layout.size();
            queue.push(base);
        }
    }
    unsafe fn slots(&self, base: usize, d: &Descriptor, visit: &mut dyn FnMut(usize)) {
        for i in 0..d.ptr_slots.1 {
            visit(unsafe { *((base + d.ptr_slots.0 + i * 8) as *const usize) });
        }
        for i in 0..d.tagged_slots.1 {
            let slot = unsafe { &*((base + d.tagged_slots.0 + i * 16) as *const TaggedSlot) };
            if usize::try_from(slot.tag)
                .ok()
                .and_then(|id| self.pointer_types.get(id))
                .copied()
                .unwrap_or(false)
            {
                visit(slot.payload as usize);
            }
        }
    }
    unsafe fn trace(&self, object: usize, visit: &mut dyn FnMut(usize)) {
        let h = unsafe { &*(object as *const Header) };
        let d = &self.descriptors[h.desc as usize];
        unsafe {
            self.slots(object, d, visit);
        }
        if let Some(e) = d.elem {
            let bytes = if let Some((i, s)) = self.locate(object) {
                self.blocks[i].sizes[s] as usize
            } else {
                self.large[&object].bytes
            };
            for offset in (e.offset..bytes).step_by(d.size) {
                let base = object + offset;
                match e.layout {
                    ElementLayout::Scalar => {}
                    ElementLayout::Pointer => visit(unsafe { *(base as *const usize) }),
                    ElementLayout::Tagged => {
                        let s = unsafe { &*(base as *const TaggedSlot) };
                        if usize::try_from(s.tag)
                            .ok()
                            .and_then(|id| self.pointer_types.get(id))
                            .copied()
                            .unwrap_or(false)
                        {
                            visit(s.payload as usize);
                        }
                    }
                    ElementLayout::Descriptor(id) => unsafe {
                        self.slots(base, &self.descriptors[id as usize], visit);
                    },
                }
            }
        }
    }
    unsafe fn drain(&mut self, queue: &mut Vec<usize>) {
        let mut edges = Vec::new();
        while let Some(p) = queue.pop() {
            edges.clear();
            unsafe {
                self.trace(p, &mut |edge| edges.push(edge));
            }
            for &edge in &edges {
                self.mark(edge, queue);
            }
        }
    }
    /// # Safety
    /// Same root/readability contract as `alloc`. No mutator may run during mark.
    pub unsafe fn collect(&mut self, roots: &dyn RootSource) {
        let start = Instant::now();
        self.cursor = 0;
        self.limit = 0;
        self.search = 0;
        self.stats.live_bytes = 0;
        for b in &mut self.blocks {
            b.sweep();
            b.marks.fill(0);
            b.lines.fill(0);
        }
        for l in self.large.values_mut() {
            l.marked = false;
        }
        let mut queue = Vec::new();
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
        self.stats.allocated_since_gc = 0;
        self.stats.budget = Self::budget_for(self.stats.live_bytes);
        self.stats.collections += 1;
        let pause = start.elapsed();
        self.stats.total_pause += pause;
        self.stats.max_pause = self.stats.max_pause.max(pause);
    }
    pub fn budget_for(live_bytes: usize) -> usize {
        live_bytes.saturating_mul(4).max(MIN_BUDGET)
    }
    /// # Safety
    /// Same contract as `collect`; stress mode collects at every safepoint.
    pub unsafe fn safepoint(&mut self, roots: &dyn RootSource) {
        if self.stress || self.stats.allocated_since_gc >= self.stats.budget {
            unsafe {
                self.collect(roots);
            }
        }
    }
    /// # Safety
    /// Root slots and traced fields must be readable, as for `collect`.
    pub unsafe fn verify(&self, roots: &dyn RootSource) {
        let check = |p| {
            assert!(
                p == 0 || self.object_start(p).is_some_and(|base| self.marked(base)),
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
                        self.trace(b.mem.ptr as usize + s * 16, &mut |p| check(p));
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
/// Phase-one write barrier. `obj` is reserved for future sticky marks.
/// # Safety
/// `slot` must be a writable, aligned pointer field of live `obj`; `value` must
/// be null or a valid managed pointer, with no conflicting Rust references.
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
