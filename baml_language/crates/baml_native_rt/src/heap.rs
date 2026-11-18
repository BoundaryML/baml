//! The allocator and the collector.
//!
//! # Objects
//!
//! [`crate::layout`] gives the layout of every object. Nothing in an object
//! is tagged: the type in its header says which of its words are references,
//! and a reference is a plain address, zero for `null`. Objects the compiler
//! places in the executable's data (string constants) have the same layout
//! and are never moved or freed.
//!
//! # Allocation
//!
//! A [`Mutator`] is one thread of execution's view of the heap. It owns a
//! span of the current space and allocates from it by bumping an address;
//! compiled code does that inline and calls [`crate::stubs::baml_rt_alloc_slow`] only when
//! the span is exhausted. Mutators share nothing but the [`Heap`] behind
//! them, which is touched only while every mutator is stopped.
//!
//! # Collection
//!
//! A stop-the-world copying collector (Cheney's algorithm): live objects are
//! copied into a fresh space and every reference to them is updated. Roots
//! are found precisely: each compiled call site has a stack map naming the
//! frame slots that hold references, and the runtime's own references live
//! in slots registered with [`Mutator::root`].
//!
//! A slice of a string does not keep the string alive. A reference to a
//! slice whose string has not been reached is set aside, and once
//! everything else has been copied each is settled: a slice of a string
//! something else reached is copied and points at the string's copy, and a
//! slice of a string nothing else reached becomes a string holding just its
//! own text. The exception is a string whose slices would together take as
//! much room as it does, which is kept for them to share.

use std::alloc::{Layout, alloc, dealloc};

use crate::layout::{
    ARRAY_DATA, ARRAY_SIZE, ENTRIES_CAPACITY, ENTRIES_RECORDS, KIND_ARRAY, KIND_ENTRIES,
    KIND_FIXED, KIND_REFS, KIND_SLICE, KIND_STRING, KIND_VALUES, SLICE_OFFSET, SLICE_PARENT,
    STORAGE_CAPACITY, STORAGE_ELEMENTS, STRING_LENGTHS, STRING_TEXT, TYPE_HEADER, TYPE_ID_MASK,
    TYPE_STRING, WORD, words,
};

/// Set in the header of an object that has been copied; the rest of the
/// header is then its new address.
const FORWARDED: usize = 1;
/// Set, while slices are settled, in the header of a slice whose text has
/// been counted and of a string whose header's high half is that count.
const COUNTED: usize = 2;
/// The low half of a header: the type id and the collector's bits.
const HEADER_LOW: usize = 0xFFFF_FFFF;
/// Where the count of a string's slices' text is in its header.
const COUNT_SHIFT: u32 = 32;

/// The first space of a heap. Spaces grow with the live data.
const INITIAL_SPACE: usize = 8 << 20;
/// The first space when every allocation collects: small, so that poisoning
/// the abandoned space each time stays cheap.
const INITIAL_STRESS_SPACE: usize = 64 << 10;
/// A space is at least this many times the live data the collection before
/// it found, so that the work of copying stays a fraction of the work of
/// allocating.
const SPACE_PER_LIVE: usize = 4;
/// What an abandoned space is filled with under stress, so that a reference
/// the collector failed to update reads as garbage.
const POISON: u8 = 0xA5;

/// What the collector needs to know about a kind of object: an entry of the
/// program's type table. `pointer_count` field offsets, in bytes from the
/// start of the object, follow the struct.
#[repr(C)]
struct TypeInfo {
    kind: u32,
    /// Bytes an object occupies, header included; fixed objects and arrays.
    size: u32,
    pointer_count: u32,
    /// The type id of an array's storage.
    storage: u32,
}

const _: () = assert!(size_of::<TypeInfo>() == TYPE_HEADER);

/// Read the word at `address`.
///
/// # Safety
///
/// `address` is word-aligned and readable.
pub unsafe fn load(address: usize) -> usize {
    // SAFETY: the caller's contract.
    unsafe { *(address as *const usize) }
}

/// Write the word at `address`.
///
/// # Safety
///
/// `address` is word-aligned and writable.
pub unsafe fn store(address: usize, value: usize) {
    // SAFETY: the caller's contract.
    unsafe { *(address as *mut usize) = value }
}

/// A contiguous block objects are allocated in.
struct Space {
    start: usize,
    size: usize,
}

impl Space {
    fn new(size: usize) -> Self {
        let layout = Layout::from_size_align(size, 2 * WORD).expect("a valid space layout");
        // SAFETY: the layout has a non-zero size.
        let start = unsafe { alloc(layout) } as usize;
        assert!(
            start != 0,
            "out of memory: cannot reserve a {size} byte heap space"
        );
        Self { start, size }
    }

    fn end(&self) -> usize {
        self.start + self.size
    }

    fn contains(&self, address: usize) -> bool {
        address >= self.start && address < self.end()
    }
}

impl Drop for Space {
    fn drop(&mut self) {
        let layout = Layout::from_size_align(self.size, 2 * WORD).expect("a valid space layout");
        // SAFETY: allocated in `new` with this layout.
        unsafe { dealloc(self.start as *mut u8, layout) };
    }
}

/// The stack maps of the program: for the return address of every call in
/// compiled code, the offsets from the caller's stack pointer of the slots
/// that hold references across that call.
struct StackMaps {
    /// `(return address, first index into offsets, count)`, sorted.
    sites: Vec<(usize, u32, u32)>,
    offsets: Vec<u32>,
}

/// A stack map entry as the 32 bits it is stored in.
fn narrow(entry: usize) -> u32 {
    u32::try_from(entry).expect("stack map entries fit 32 bits")
}

impl StackMaps {
    /// Decode the table the compiler emits: a count of functions, then for
    /// each its address, a count of call sites, and for each site its return
    /// address as an offset into the function, a count of slots, and the
    /// slots. Every entry is one word.
    ///
    /// # Safety
    ///
    /// `table` is the address of a table in that format.
    unsafe fn decode(table: usize) -> Self {
        let mut cursor = table;
        let mut next = || {
            // SAFETY: the cursor stays inside the table by its own counts.
            let word = unsafe { load(cursor) };
            cursor += WORD;
            word
        };
        let mut maps = Self {
            sites: Vec::new(),
            offsets: Vec::new(),
        };
        for _ in 0..next() {
            let function = next();
            for _ in 0..next() {
                let return_address = function + next();
                let count = next();
                let first = maps.offsets.len();
                for _ in 0..count {
                    maps.offsets.push(narrow(next()));
                }
                maps.sites
                    .push((return_address, narrow(first), narrow(count)));
            }
        }
        maps.sites.sort_unstable();
        maps
    }

    fn slots(&self, return_address: usize) -> &[u32] {
        match self
            .sites
            .binary_search_by_key(&return_address, |site| site.0)
        {
            Ok(index) => {
                let (_, first, count) = self.sites[index];
                &self.offsets[first as usize..(first + count) as usize]
            }
            Err(_) => &[],
        }
    }
}

/// What the compiled program tells the runtime about itself.
#[derive(Clone, Copy)]
pub struct Program {
    /// The address of its stack map table.
    pub stack_maps: usize,
    /// The address of its type table.
    pub types: usize,
    /// The address of its table of calls with a handler.
    pub unwind: usize,
    /// The address of its function table.
    pub functions: usize,
}

/// The memory every mutator allocates from. Read and written only while
/// every mutator is stopped.
struct Heap {
    space: Space,
    /// The space the collection before last abandoned, kept to copy into
    /// again: memory already touched costs no page faults.
    spare: Option<Space>,
    /// The size of the space the next collection copies into, unless more
    /// is in use by then.
    next_space: usize,
    stack_maps: StackMaps,
    types: usize,
    /// Collect on every allocation, and poison what a collection abandons.
    stress: bool,
}

/// One thread of execution's allocation state and roots. Compiled code holds
/// its address in a dedicated register and reads the first two fields
/// directly.
#[repr(C)]
pub struct Mutator {
    /// Where the next object goes.
    alloc_ptr: usize,
    /// The end of the span compiled code may bump into without calling the
    /// runtime. Under stress it equals `alloc_ptr`, so every allocation
    /// calls.
    alloc_limit: usize,
    /// Compiled functions call the runtime on entry when the stack pointer
    /// is below this. Zero: never.
    stack_limit: usize,
    /// Compiled loops call the runtime before their next iteration when
    /// this is not zero.
    poll: u32,
    /// What the allocation slow path is asked for and what it answers.
    scratch: usize,
    /// The seed of this process's hashes: see [`crate::map`].
    hash_seed: u64,
    /// The true end of this mutator's span.
    span_end: usize,
    heap: Heap,
    /// Where a collection starts walking this stack: the stack pointer, the
    /// return address and the frame pointer of the compiled function that
    /// last called the runtime, as they were at that call.
    anchor: Anchor,
    /// The frame that called into compiled code: where the walk stops.
    base_frame: usize,
    /// Addresses of the runtime's own slots holding references.
    roots: Vec<usize>,
    /// Where a throw finds its handler.
    pub(crate) unwind: crate::unwind::Unwinder,
}

const _: () = {
    assert!(std::mem::offset_of!(Mutator, alloc_ptr) == crate::layout::MUTATOR_ALLOC_PTR);
    assert!(std::mem::offset_of!(Mutator, alloc_limit) == crate::layout::MUTATOR_ALLOC_LIMIT);
    assert!(std::mem::offset_of!(Mutator, stack_limit) == crate::layout::MUTATOR_STACK_LIMIT);
    assert!(std::mem::offset_of!(Mutator, poll) == crate::layout::MUTATOR_POLL);
    assert!(std::mem::offset_of!(Mutator, scratch) == crate::layout::MUTATOR_SCRATCH);
    assert!(std::mem::offset_of!(Mutator, hash_seed) == crate::layout::MUTATOR_HASH_SEED);
};

/// The top compiled frame of a stack, recorded where compiled code calls the
/// runtime.
#[derive(Clone, Copy, Default)]
struct Anchor {
    /// The compiled function's stack pointer at the call: what its stack
    /// map offsets are relative to.
    sp: usize,
    /// The return address of the call: what selects the stack map.
    pc: usize,
    /// The compiled function's frame pointer: the rest of the chain.
    fp: usize,
}

impl Mutator {
    /// A mutator with a heap of its own.
    ///
    /// # Safety
    ///
    /// `program` holds the addresses of the compiler's tables.
    pub unsafe fn new(program: Program, stress: bool) -> Self {
        let size = if stress {
            INITIAL_STRESS_SPACE
        } else {
            INITIAL_SPACE
        };
        let space = Space::new(size);
        let mut mutator = Self {
            alloc_ptr: space.start,
            alloc_limit: 0,
            stack_limit: 0,
            poll: 0,
            scratch: 0,
            hash_seed: crate::map::seed(),
            span_end: space.end(),
            heap: Heap {
                space,
                spare: None,
                next_space: size,
                // SAFETY: the caller's contract.
                stack_maps: unsafe { StackMaps::decode(program.stack_maps) },
                types: program.types,
                stress,
            },
            anchor: Anchor::default(),
            base_frame: 0,
            roots: Vec::new(),
            unwind: crate::unwind::Unwinder::new(program.unwind, program.functions),
        };
        mutator.publish_limit();
        mutator
    }

    fn publish_limit(&mut self) {
        self.alloc_limit = if self.heap.stress {
            self.alloc_ptr
        } else {
            self.span_end
        };
    }

    /// Record where this stack's compiled frames end: at `frame`, the frame
    /// pointer of the function about to call into compiled code.
    #[cfg(not(test))]
    pub fn set_base_frame(&mut self, frame: usize) {
        self.base_frame = frame;
    }

    /// Record the top of this stack of compiled frames, given `frame`, the
    /// frame pointer of a runtime function compiled code called directly.
    ///
    /// Only the frame record the platform's ABI defines is read: the
    /// caller's frame pointer and the return address. Nothing else about
    /// the runtime function's frame matters, so a boundary that switches
    /// stacks can record the same three words itself.
    pub fn enter(&mut self, frame: usize) {
        // SAFETY: `frame` is the frame record of a function compiled code
        // called.
        self.anchor = unsafe {
            Anchor {
                sp: frame + 2 * WORD,
                pc: load(frame + WORD),
                fp: load(frame),
            }
        };
    }

    /// The top compiled frame: the return address into it and its frame
    /// pointer, as [`Self::enter`] recorded them.
    pub(crate) fn top_frame(&self) -> (usize, usize) {
        (self.anchor.pc, self.anchor.fp)
    }

    /// The frame pointer of the function that called into compiled code.
    pub(crate) fn base_frame(&self) -> usize {
        self.base_frame
    }

    /// A loop found the poll word set. Nothing sets it yet.
    pub fn poll(&mut self) {
        self.poll = 0;
    }

    /// Keep the reference in `*slot` alive and current across allocations
    /// until the matching [`Self::unroot`].
    pub fn root(&mut self, slot: &mut usize) {
        self.roots.push(std::ptr::from_mut(slot) as usize);
    }

    /// Like [`Self::root`], for a slot known by address.
    pub fn root_at(&mut self, slot: usize) {
        self.roots.push(slot);
    }

    /// Drop the last `count` roots.
    pub fn unroot(&mut self, count: usize) {
        self.roots.truncate(self.roots.len() - count);
    }

    /// The name of variant `index` of the enum whose boxes have the type
    /// `enum_type`.
    pub fn variant_name(&self, enum_type: usize, index: usize) -> &'static str {
        let types = self.heap.types;
        // SAFETY: an enum's type id is an entry of the type table whose
        // fourth word locates its names, which live in the executable's
        // data.
        unsafe {
            let names = types + (*((types + enum_type) as *const TypeInfo)).storage as usize;
            let word = |position: usize| *((names + position * 4) as *const u32) as usize;
            assert!(index < word(0), "an enum value names one of its variants");
            let (start, length) = (word(1 + 2 * index), word(2 + 2 * index));
            std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                (types + start) as *const u8,
                length,
            ))
        }
    }

    /// Whether values of the type a call names by `type_id` are references:
    /// every type but `int`, `float`, `bool` and the enums, whose ids are
    /// those of their boxes. Zero names a type whose values share no one
    /// id, all of which are references.
    pub fn is_reference(&self, type_id: usize) -> bool {
        match type_id {
            0 => true,
            crate::layout::TYPE_BOX_INT
            | crate::layout::TYPE_BOX_FLOAT
            | crate::layout::TYPE_BOX_BOOL => false,
            // SAFETY: any other id is an entry of the type table. An enum's
            // boxes hold no reference and locate the enum's names.
            _ => unsafe {
                let info = &*((self.heap.types + type_id) as *const TypeInfo);
                !(info.kind == KIND_FIXED && info.pointer_count == 0 && info.storage != 0)
            },
        }
    }

    /// The type id of the storage behind arrays of type `array_type`.
    pub fn storage_type(&self, array_type: usize) -> usize {
        // SAFETY: an array's type id is an entry of the type table.
        unsafe { (*((self.heap.types + array_type) as *const TypeInfo)).storage as usize }
    }

    /// Allocate `size` bytes (a whole number of words) and return their
    /// address. The memory is not initialized. May collect: every reference
    /// the caller holds must be rooted.
    pub fn allocate(&mut self, size: usize) -> usize {
        debug_assert!(size.is_multiple_of(WORD));
        if self.heap.stress || self.alloc_ptr + size > self.span_end {
            self.collect(size);
        }
        let address = self.alloc_ptr;
        self.alloc_ptr += size;
        self.publish_limit();
        address
    }

    /// Allocate an object of `size` bytes whose type id is `type_id`.
    pub fn allocate_object(&mut self, type_id: usize, size: usize) -> usize {
        let object = self.allocate(size);
        // SAFETY: `object` is `size >= WORD` fresh bytes.
        unsafe { store(object, type_id) };
        object
    }

    /// Copy everything reachable into another space, leaving room for
    /// `needed` more bytes, and abandon the old one.
    fn collect(&mut self, needed: usize) {
        let used = self.alloc_ptr - self.heap.space.start;
        if self.heap.stress {
            self.copy_into(used + needed);
            return;
        }
        // Everything may be live, so the other space holds at least all of
        // it. When that is the size the spaces already have, the one
        // abandoned last time is the one copied into now.
        self.copy_into(used);
        let live = self.alloc_ptr - self.heap.space.start;
        // A space mostly full of live data is collected again almost at
        // once, so the next one is sized by what survived this time.
        self.heap.next_space = self.heap.space.size.max((live + needed) * SPACE_PER_LIVE);
        if live + needed > self.heap.space.size {
            // What survived leaves no room for what was asked for: move
            // again, into a space sized for it.
            self.copy_into(live + needed);
        }
        if (self.heap.spare.as_ref()).is_some_and(|spare| spare.size < self.heap.next_space) {
            self.heap.spare = None;
        }
    }

    /// Copy everything reachable into a space of at least `least` bytes and
    /// of the size the next space is to have, and allocate from there.
    fn copy_into(&mut self, least: usize) {
        let size = words(self.heap.next_space.max(least));
        let to = match self.heap.spare.take() {
            Some(spare) if spare.size >= size => spare,
            _ => Space::new(size),
        };
        let mut copy = Copy::new(&self.heap.space, to, self.heap.types);
        // SAFETY: the stack between the anchor and `base_frame` is this
        // mutator's compiled frames, described by the stack maps, and every
        // registered root is a live slot.
        unsafe {
            copy.stack(&self.heap.stack_maps, self.anchor, self.base_frame);
            for &slot in &self.roots {
                copy.forward(slot);
            }
            copy.finish();
        }
        let live = copy.free - copy.to.start;
        debug_assert!(copy.free <= copy.to.end());
        let to = copy.to;
        let from = std::mem::replace(&mut self.heap.space, to);
        if self.heap.stress {
            // SAFETY: the space is owned and no live object remains in it.
            unsafe { std::ptr::write_bytes(from.start as *mut u8, POISON, from.size) };
        } else {
            self.heap.spare = Some(from);
        }
        self.alloc_ptr = self.heap.space.start + live;
        self.span_end = self.heap.space.end();
        self.publish_limit();
    }
}

/// One copying pass from a space into a new one.
struct Copy<'a> {
    from: &'a Space,
    to: Space,
    types: usize,
    /// Where the next copied object goes.
    free: usize,
    /// The slots found holding a reference to a slice of a string that had
    /// not been reached.
    parked: Vec<usize>,
}

impl<'a> Copy<'a> {
    fn new(from: &'a Space, to: Space, types: usize) -> Self {
        let free = to.start;
        Self {
            from,
            to,
            types,
            free,
            parked: Vec::new(),
        }
    }

    /// The type of an object from its header.
    ///
    /// # Safety
    ///
    /// `header` is the header of an object that has not been forwarded.
    unsafe fn info(&self, header: usize) -> &'static TypeInfo {
        // SAFETY: a type id is the offset of an entry of the type table,
        // which lives in the executable's data.
        unsafe { &*((self.types + (header & TYPE_ID_MASK)) as *const TypeInfo) }
    }

    /// How many bytes the object at `object` occupies.
    ///
    /// # Safety
    ///
    /// `object` is a live object of type `info`.
    unsafe fn size(object: usize, info: &TypeInfo) -> usize {
        // SAFETY: storage keeps its capacity, and a string its byte length
        // in the low half, in its second word.
        unsafe {
            match info.kind {
                KIND_FIXED | KIND_ARRAY | KIND_SLICE => info.size as usize,
                KIND_VALUES | KIND_REFS => {
                    STORAGE_ELEMENTS + load(object + STORAGE_CAPACITY) * WORD
                }
                KIND_STRING => STRING_TEXT + words(load(object + STRING_LENGTHS) & 0xFFFF_FFFF),
                KIND_ENTRIES => crate::layout::entries_size(load(object + ENTRIES_CAPACITY)),
                kind => unreachable!("an object of unknown kind {kind}"),
            }
        }
    }

    /// Copy the `size` bytes of the object at `object`, which has not been
    /// copied, and leave the copy's address in its place.
    ///
    /// # Safety
    ///
    /// `object` is an object of `size` bytes in the space being copied
    /// from.
    unsafe fn copy_out(&mut self, object: usize, size: usize) -> usize {
        let copy = self.free;
        // SAFETY: the caller's contract, and the space being copied into
        // has room for everything in the one being copied from.
        unsafe {
            std::ptr::copy_nonoverlapping(object as *const u8, copy as *mut u8, size);
            store(object, copy | FORWARDED);
        }
        self.free += size;
        copy
    }

    /// The address of the copy of the object at `object`, which is not a
    /// slice, copying it first if this is the first reference to it seen.
    /// An address outside the space being collected, null included, is
    /// returned as it is.
    ///
    /// # Safety
    ///
    /// `object` is null or the address of an object that is not a slice.
    unsafe fn copied(&mut self, object: usize) -> usize {
        if !self.from.contains(object) {
            return object;
        }
        // SAFETY: every object in the space being copied from has a header
        // that is a type id or a forwarding address.
        unsafe {
            let header = load(object);
            if header & FORWARDED != 0 {
                return header & !FORWARDED;
            }
            let size = Self::size(object, self.info(header));
            self.copy_out(object, size)
        }
    }

    /// Make the reference in `*slot` point at the copy of its object. A
    /// reference to a slice of a string not reached yet is left as it is
    /// until [`Self::settle_slices`].
    ///
    /// # Safety
    ///
    /// `slot` is the address of a word that holds null or a reference, and
    /// stays where it is until the collection ends.
    unsafe fn forward(&mut self, slot: usize) {
        // SAFETY: the caller's contract, and every object in the space
        // being copied from has a header that is a type id or a forwarding
        // address.
        unsafe {
            let object = load(slot);
            if !self.from.contains(object) {
                return;
            }
            let header = load(object);
            if header & FORWARDED != 0 {
                store(slot, header & !FORWARDED);
                return;
            }
            let info = self.info(header);
            if info.kind == KIND_SLICE {
                let parent = load(object + SLICE_PARENT);
                if self.from.contains(parent) {
                    let reached = load(parent);
                    if reached & FORWARDED == 0 {
                        self.parked.push(slot);
                        return;
                    }
                    store(object + SLICE_PARENT, reached & !FORWARDED);
                }
            }
            let size = Self::size(object, info);
            store(slot, self.copy_out(object, size));
        }
    }

    /// Settle every reference set aside: by now whatever is going to reach
    /// a string directly has.
    ///
    /// Nothing here needs scanning afterwards: a string holds no
    /// reference, and a slice copied here already points at a copy.
    ///
    /// # Safety
    ///
    /// Everything reachable other than through the slots set aside has
    /// been copied.
    unsafe fn settle_slices(&mut self) {
        let parked = std::mem::take(&mut self.parked);
        // SAFETY: each slot holds a reference to a slice in the space being
        // copied from; a slice's string holds its text, and the headers
        // written here are of objects that space is abandoned with.
        unsafe {
            // How much room the text of each unreached string's slices
            // would take as strings of their own, each slice counted once.
            for &slot in &parked {
                let slice = load(slot);
                let header = load(slice);
                if header & (FORWARDED | COUNTED) != 0 {
                    continue;
                }
                let parent = load(slice + SLICE_PARENT);
                let mut counted = load(parent);
                if counted & FORWARDED != 0 {
                    continue;
                }
                if counted & COUNTED == 0 {
                    counted = (counted & HEADER_LOW) | COUNTED;
                }
                let count = (counted >> COUNT_SHIFT) + Self::text_size(slice);
                store(slice, header | COUNTED);
                store(
                    parent,
                    (counted & HEADER_LOW) | (count.min(HEADER_LOW) << COUNT_SHIFT),
                );
            }
            for &slot in &parked {
                let slice = load(slot);
                let header = load(slice);
                if header & FORWARDED != 0 {
                    store(slot, header & !FORWARDED);
                    continue;
                }
                let parent = load(slice + SLICE_PARENT);
                let mut reached = load(parent);
                if reached & FORWARDED == 0 {
                    let (count, size) = (reached >> COUNT_SHIFT, Self::text_size(parent));
                    if count < size && count < HEADER_LOW {
                        store(slot, self.flatten(slice));
                        continue;
                    }
                    // Its slices are no smaller than it is. The count goes,
                    // and so would a hash kept where it was.
                    store(parent, TYPE_STRING);
                    reached = self.copy_out(parent, size) | FORWARDED;
                }
                store(slice, header & !COUNTED);
                store(slice + SLICE_PARENT, reached & !FORWARDED);
                let size = Self::size(slice, self.info(header));
                store(slot, self.copy_out(slice, size));
            }
        }
    }

    /// The bytes a string holding the text of the string at `string` takes.
    ///
    /// # Safety
    ///
    /// `string` is a string or a slice.
    unsafe fn text_size(string: usize) -> usize {
        // SAFETY: both keep their byte length in the low half of their
        // second word.
        unsafe { STRING_TEXT + words(load(string + STRING_LENGTHS) & HEADER_LOW) }
    }

    /// Copy the text of the slice at `slice`, whose string is not copied,
    /// into a new string, which every later reference to the slice finds in
    /// its place.
    ///
    /// # Safety
    ///
    /// `slice` is a slice in the space being copied from that has not been
    /// copied, of a string in that space.
    unsafe fn flatten(&mut self, slice: usize) -> usize {
        let flat = self.free;
        // SAFETY: the caller's contract. The strings made here of one
        // string's slices take less room than that string, which is not
        // copied, so the space being copied into has room.
        unsafe {
            let lengths = load(slice + STRING_LENGTHS);
            let size = Self::text_size(slice);
            let text = load(slice + SLICE_PARENT) + STRING_TEXT + load(slice + SLICE_OFFSET);
            store(flat, TYPE_STRING);
            // The last word first, for the zeros after the text.
            store(flat + size - WORD, 0);
            store(flat + STRING_LENGTHS, lengths);
            std::ptr::copy_nonoverlapping(
                text as *const u8,
                (flat + STRING_TEXT) as *mut u8,
                lengths & HEADER_LOW,
            );
            store(slice, flat | FORWARDED);
            self.free += size;
        }
        flat
    }

    /// Forward the references in every compiled frame between `top` and
    /// `base`. Each frame record is the saved frame pointer of the caller
    /// followed by the return address into it, and the caller's stack
    /// pointer at the call is just above the record.
    ///
    /// # Safety
    ///
    /// `top` is all zero or the top compiled frame of a stack, and `base`
    /// the frame pointer of the function that called into compiled code on
    /// that stack.
    unsafe fn stack(&mut self, maps: &StackMaps, top: Anchor, base: usize) {
        let Anchor {
            mut sp,
            mut pc,
            mut fp,
        } = top;
        while sp != 0 {
            // SAFETY: `sp` is a compiled frame's stack pointer at the call
            // returning to `pc`, and `fp` its frame record.
            unsafe {
                for &offset in maps.slots(pc) {
                    self.forward(sp + offset as usize);
                }
                if fp >= base {
                    break;
                }
                // A caller's stack pointer at a call is just above the
                // callee's frame record: arguments travel in registers or
                // in the caller's own frame, and no function here makes a
                // tail call, which is what would move the record.
                sp = fp + 2 * WORD;
                pc = load(fp + WORD);
                fp = load(fp);
            }
        }
    }

    /// Copy everything reachable from what has been copied so far.
    ///
    /// # Safety
    ///
    /// Every object copied so far is intact.
    unsafe fn finish(&mut self) {
        let mut scan = self.to.start;
        while scan < self.free {
            // SAFETY: `scan` is the start of a copied object.
            unsafe {
                let info = self.info(load(scan));
                match info.kind {
                    KIND_FIXED => {
                        let offsets =
                            (std::ptr::from_ref(info) as usize + TYPE_HEADER) as *const u32;
                        for index in 0..info.pointer_count as usize {
                            self.forward(scan + *offsets.add(index) as usize);
                        }
                    }
                    // An array holds the address of its first element, which
                    // is inside its storage object.
                    KIND_ARRAY => {
                        let data = load(scan + ARRAY_DATA);
                        if data != 0 {
                            let storage = self.copied(data - STORAGE_ELEMENTS);
                            store(scan + ARRAY_DATA, storage + STORAGE_ELEMENTS);
                        }
                    }
                    KIND_REFS => {
                        for index in 0..load(scan + STORAGE_CAPACITY) {
                            self.forward(scan + STORAGE_ELEMENTS + index * WORD);
                        }
                    }
                    // A map's entries: the references of each record, at
                    // the same offsets in every one. A record not in use
                    // is zero.
                    KIND_ENTRIES => {
                        let offsets =
                            (std::ptr::from_ref(info) as usize + TYPE_HEADER) as *const u32;
                        let stride = info.size as usize;
                        for record in 0..load(scan + ENTRIES_CAPACITY) {
                            for index in 0..info.pointer_count as usize {
                                self.forward(
                                    scan + ENTRIES_RECORDS
                                        + record * stride
                                        + *offsets.add(index) as usize,
                                );
                            }
                        }
                    }
                    _ => {}
                }
                scan += Self::size(scan, info);
            }
        }
        // SAFETY: the scan is done.
        unsafe { self.settle_slices() };
    }
}

const _: () = assert!(ARRAY_SIZE == 3 * WORD);

/// The frame pointer of the function this is inlined into.
#[expect(
    clippy::inline_always,
    reason = "it reads a register of whichever function it is in"
)]
#[inline(always)]
pub fn current_frame() -> usize {
    let frame: usize;
    // SAFETY: reads one register.
    unsafe {
        #[cfg(target_arch = "aarch64")]
        std::arch::asm!("mov {}, x29", out(reg) frame, options(nomem, nostack, preserves_flags));
        #[cfg(target_arch = "x86_64")]
        std::arch::asm!("mov {}, rbp", out(reg) frame, options(nomem, nostack, preserves_flags));
    }
    frame
}

/// The slow path of allocation in compiled code: the mutator's span cannot
/// take the bytes asked for in its scratch word, which is left holding the
/// address of that much uninitialized memory. `frame` is the frame record
/// of the stub compiled code called.
pub(crate) extern "C" fn alloc_slow(mutator: &mut Mutator, frame: usize) {
    mutator.enter(frame);
    mutator.scratch = mutator.allocate(mutator.scratch);
}

/// A compiled loop found the poll word set.
pub(crate) extern "C" fn poll_slow(mutator: &mut Mutator, frame: usize) {
    mutator.enter(frame);
    mutator.poll();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        array::{baml_rt_array_new, baml_rt_array_push_slow},
        layout::{
            ARRAY_LENGTH, FIRST_CLASS_TYPE, RUNTIME_TYPES, SLICE_SIZE, TYPE_ARRAY_OF_REFS,
            TYPE_STRING_SLICE, field_offset, instance_size,
        },
        string,
    };

    /// A stack map table with no functions.
    static NO_STACK_MAPS: usize = 0;

    /// The id of a class of two slots, an int and a reference to another
    /// instance, in [`types`].
    const LINK: usize = FIRST_CLASS_TYPE;

    /// A type table holding the fixed types and `LINK`.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the sizes and offsets of a two-slot class"
    )]
    fn types() -> &'static [u32] {
        let mut table = RUNTIME_TYPES.to_vec();
        table.extend([
            KIND_FIXED,
            instance_size(2) as u32,
            1,
            0,
            field_offset(1) as u32,
            0,
        ]);
        table.leak()
    }

    fn mutator(stress: bool) -> Mutator {
        let program = Program {
            stack_maps: std::ptr::from_ref(&NO_STACK_MAPS) as usize,
            types: types().as_ptr() as usize,
            unwind: 0,
            functions: 0,
        };
        // SAFETY: both tables are in the compiler's formats.
        unsafe { Mutator::new(program, stress) }
    }

    /// Push a link holding `value` onto the list in `*head`.
    fn push_link(mutator: &mut Mutator, head: &mut usize, value: usize) {
        let link = mutator.allocate_object(LINK, instance_size(2));
        // SAFETY: `link` is a fresh instance of two slots, and `head` was
        // kept current across the allocation.
        unsafe {
            store(link + field_offset(0), value);
            store(link + field_offset(1), *head);
        }
        *head = link;
    }

    fn list_values(mut link: usize) -> Vec<usize> {
        let mut values = Vec::new();
        while link != 0 {
            // SAFETY: `link` is a live instance of `LINK`.
            unsafe {
                values.push(load(link + field_offset(0)));
                link = load(link + field_offset(1));
            }
        }
        values
    }

    #[test]
    fn a_rooted_list_survives_a_collection_per_allocation() {
        let mut mutator = mutator(true);
        let mut head = 0;
        mutator.root(&mut head);
        for value in 0..500 {
            push_link(&mut mutator, &mut head, value);
        }
        mutator.unroot(1);
        assert_eq!(list_values(head), (0..500).rev().collect::<Vec<_>>());
    }

    #[test]
    fn the_heap_grows_past_its_first_space() {
        let mut mutator = mutator(false);
        let mut head = 0;
        mutator.root(&mut head);
        let links = 2 * INITIAL_SPACE / instance_size(2);
        for value in 0..links {
            push_link(&mut mutator, &mut head, value);
        }
        mutator.unroot(1);
        assert_eq!(list_values(head).len(), links);
        assert!(mutator.heap.space.size > INITIAL_SPACE);
    }

    #[test]
    fn an_array_of_strings_grows_under_collection() {
        let mut mutator = mutator(true);
        // SAFETY: the calls follow the contracts of compiled code, with the
        // array rooted here in place of a stack map.
        unsafe {
            let mut array = baml_rt_array_new(&mut mutator, TYPE_ARRAY_OF_REFS, 0);
            mutator.root(&mut array);
            for index in 0..100 {
                let text = string::from_str(&mut mutator, &format!("item {index}"));
                baml_rt_array_push_slow(&mut mutator, array, text);
            }
            mutator.unroot(1);
            assert_eq!(load(array + ARRAY_LENGTH), 100);
            let data = load(array + ARRAY_DATA);
            for index in 0..100 {
                assert_eq!(
                    string::text(load(data + index * WORD)),
                    format!("item {index}")
                );
            }
        }
    }

    /// A slice of bytes `from..to` of the string in `*parent`, which is all
    /// ASCII.
    fn slice(mutator: &mut Mutator, parent: &mut usize, from: usize, to: usize) -> usize {
        mutator.root(parent);
        let slice = mutator.allocate_object(TYPE_STRING_SLICE, SLICE_SIZE);
        mutator.unroot(1);
        let length = to - from;
        // SAFETY: a fresh slice of four words.
        unsafe {
            store(slice + STRING_LENGTHS, length | (length << 32));
            store(slice + SLICE_PARENT, *parent);
            store(slice + SLICE_OFFSET, from);
        }
        slice
    }

    fn is_slice(string: usize) -> bool {
        // SAFETY: `string` is a live string.
        unsafe { load(string) & TYPE_ID_MASK == TYPE_STRING_SLICE }
    }

    const LETTERS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

    #[test]
    fn a_slice_alone_becomes_a_string_of_its_own_text() {
        let mut mutator = mutator(true);
        let mut parent = string::from_str(&mut mutator, &LETTERS.repeat(4));
        let mut part = slice(&mut mutator, &mut parent, 5, 45);
        let mut again = part;
        mutator.root(&mut part);
        mutator.root(&mut again);
        mutator.allocate(WORD);
        mutator.unroot(2);
        assert!(!is_slice(part));
        assert_eq!(part, again);
        // SAFETY: `part` was kept current.
        assert_eq!(unsafe { string::text(part) }, &LETTERS.repeat(4)[5..45]);
        let live = mutator.alloc_ptr - mutator.heap.space.start;
        assert_eq!(live, STRING_TEXT + words(40) + WORD);
    }

    #[test]
    fn a_slice_follows_a_string_reached_before_or_after_it() {
        for string_first in [false, true] {
            let mut mutator = mutator(true);
            let mut parent = string::from_str(&mut mutator, &LETTERS.repeat(4));
            let mut part = slice(&mut mutator, &mut parent, 5, 45);
            if string_first {
                mutator.root(&mut parent);
                mutator.root(&mut part);
            } else {
                mutator.root(&mut part);
                mutator.root(&mut parent);
            }
            mutator.allocate(WORD);
            mutator.unroot(2);
            assert!(is_slice(part));
            // SAFETY: both were kept current.
            unsafe {
                assert_eq!(load(part + SLICE_PARENT), parent);
                assert_eq!(string::text(part), &LETTERS.repeat(4)[5..45]);
            }
        }
    }

    #[test]
    fn slices_as_large_as_their_string_share_it() {
        let mut mutator = mutator(true);
        let text = LETTERS.repeat(2);
        let mut parent = string::from_str(&mut mutator, &text);
        let mut parts = [0; 3];
        for (index, part) in parts.iter_mut().enumerate() {
            *part = slice(&mut mutator, &mut parent, index, index + 60);
            mutator.root(part);
        }
        mutator.allocate(WORD);
        mutator.unroot(3);
        // SAFETY: the parts were kept current.
        unsafe {
            let shared = load(parts[0] + SLICE_PARENT);
            assert_eq!(string::text(shared), text);
            assert_eq!(load(shared), TYPE_STRING);
            for (index, &part) in parts.iter().enumerate() {
                assert!(is_slice(part));
                assert_eq!(load(part + SLICE_PARENT), shared);
                assert_eq!(string::text(part), &text[index..index + 60]);
            }
        }
    }

    #[test]
    fn slices_smaller_than_their_string_leave_it_behind() {
        let mut mutator = mutator(true);
        let text = LETTERS.repeat(8);
        let mut parent = string::from_str(&mut mutator, &text);
        let mut parts = [0; 3];
        for (index, part) in parts.iter_mut().enumerate() {
            *part = slice(&mut mutator, &mut parent, index * 100, index * 100 + 40);
            mutator.root(part);
        }
        mutator.allocate(WORD);
        mutator.unroot(3);
        for (index, &part) in parts.iter().enumerate() {
            assert!(!is_slice(part));
            // SAFETY: the parts were kept current.
            assert_eq!(
                unsafe { string::text(part) },
                &text[index * 100..index * 100 + 40]
            );
        }
        let live = mutator.alloc_ptr - mutator.heap.space.start;
        assert_eq!(live, 3 * (STRING_TEXT + words(40)) + WORD);
    }
}
