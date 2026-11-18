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
    ARRAY_DATA, ARRAY_SIZE, ENTRIES_CAPACITY, ENTRIES_RECORDS, FUTURE_ERROR, FUTURE_FLAGS,
    FUTURE_READY, FUTURE_SETTLED, FUTURE_STATE, FUTURE_TRACE, FUTURE_VALUE, FUTURE_VALUE_IS_REF,
    KIND_ARRAY, KIND_ENTRIES, KIND_FIXED, KIND_FUTURE, KIND_OWNED, KIND_REFS, KIND_SLICE,
    KIND_STRING, KIND_VALUES, OWNED_KIND, OWNED_SIZE, OWNED_VALUE, SLICE_OFFSET, SLICE_PARENT,
    STORAGE_CAPACITY, STORAGE_ELEMENTS, STRING_LENGTHS, STRING_TEXT, TYPE_HEADER, TYPE_ID_MASK,
    TYPE_OWNED, TYPE_STRING, WORD, words,
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
/// The fewest owned Rust values made since a collection that bring on the
/// next one. An owned value is dropped only by the collection that finds
/// its object dead, and what it holds is outside the heap, so a program
/// that makes many and little else would never fill a space and never drop
/// one. A value can be an open file or socket, and a process may by
/// default have 256 of those open, so the floor is half that.
const OWNED_FLOOR: usize = 128;
/// Past the floor, owned values bring on a collection once this many times
/// the ones the last collection found alive have been made since: the work
/// of collecting stays in proportion to what is made, as for a space.
const OWNED_PER_LIVE: usize = 2;

/// What the runtime knows of the type of a Rust value in the heap: how to
/// drop one the heap owns, and which type it is, for a native that expects
/// another to say so. A value's object holds the address of one of these.
#[repr(C)]
pub struct RustKind {
    /// Give up the heap's hold on the value at this address. Plain data
    /// has nothing to drop, and its kind's is never called.
    pub(crate) drop: unsafe fn(usize),
    pub(crate) id: fn() -> std::any::TypeId,
    pub(crate) name: fn() -> &'static str,
}

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
    /// `(return address, first index into offsets, count)`, sorted. A
    /// call across which no slot holds a reference is here only in a build
    /// with checks, with a count of zero.
    sites: Vec<(usize, u32, u32)>,
    offsets: Vec<u32>,
    /// The address of the program's function table, which says where
    /// compiled code is.
    #[cfg(checks)]
    functions: usize,
}

/// An entry of one of the compiler's tables as the 32 bits it is stored
/// in.
fn narrow(entry: usize) -> u32 {
    u32::try_from(entry).expect("table entries fit 32 bits")
}

impl StackMaps {
    /// Decode the table the compiler emits: a count of functions, then for
    /// each its address, a count of call sites, and for each site its return
    /// address as an offset into the function, a count of slots, and the
    /// slots. Every entry is one word.
    ///
    /// The calls across which no slot holds a reference follow, and only a
    /// build with checks reads them: a count of functions, then for each
    /// its address, a count of such calls, and the return address of each
    /// as an offset into the function. With them every call in compiled
    /// code has an entry, so a return address into compiled code that has
    /// none is a map the compiler lost, and not a frame without roots.
    ///
    /// # Safety
    ///
    /// `program` holds the addresses of tables in the compiler's formats.
    unsafe fn decode(program: Program) -> Self {
        let table = program.stack_maps;
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
            #[cfg(checks)]
            functions: program.functions,
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
        #[cfg(checks)]
        for _ in 0..next() {
            let function = next();
            for _ in 0..next() {
                maps.sites.push((function + next(), 0, 0));
            }
        }
        maps.sites.sort_unstable();
        maps
    }

    /// The slots of the frame that a call returning to `return_address`
    /// was made from. None for a frame of the runtime, which holds its
    /// references in registered roots.
    ///
    /// A build with checks has an entry for every call in compiled code,
    /// and ends the program at a return address into compiled code that
    /// has none: the compiler lost the map, and with it the frame's roots.
    fn slots(&self, return_address: usize) -> &[u32] {
        match self
            .sites
            .binary_search_by_key(&return_address, |site| site.0)
        {
            Ok(index) => {
                let (_, first, count) = self.sites[index];
                &self.offsets[first as usize..(first + count) as usize]
            }
            Err(_) => {
                #[cfg(checks)]
                if let Some(function) = crate::unwind::function_name(self.functions, return_address)
                {
                    crate::sched::fatal(&format!(
                        "a compiled frame has no stack map: the call that returns to \
                         {return_address:#x}, in `{function}`. The collector cannot tell which \
                         of the frame's slots hold references."
                    ));
                }
                &[]
            }
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
    /// Every object that owns a Rust value, dead ones included until the
    /// next collection drops their values.
    owned: Vec<usize>,
    /// How many of them were made since the last collection, and how many
    /// bring on the next.
    owned_since: usize,
    owned_trigger: usize,
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
    /// The task this worker is running, or null on the worker's own stack.
    pub(crate) cur_task: *mut crate::sched::Task,
    /// Reserved for the telemetry node of the running task.
    tele_node: usize,
    /// Where the worker's own stack resumes when a task leaves for it.
    g0_sp: usize,
    /// Where a collection starts walking the running task's stack: the
    /// stack pointer, the return address and the frame pointer of the
    /// compiled function that last called the runtime, as they were at that
    /// call.
    anchor: Anchor,
    /// The true end of this mutator's span.
    span_end: usize,
    heap: Heap,
    /// The tasks, and what runs them.
    pub(crate) sched: crate::sched::Sched,
    /// Addresses of the runtime's own slots holding references.
    roots: Vec<usize>,
    /// Where a throw finds its handler.
    pub(crate) unwind: crate::unwind::Unwinder,
    /// Whether a runtime function has moved to the worker's own stack.
    on_worker_stack: bool,
    /// The `async` native this worker is polling, for a panic inside it
    /// to name.
    pub(crate) polling: Option<&'static str>,
}

const _: () = {
    assert!(std::mem::offset_of!(Mutator, alloc_ptr) == crate::layout::MUTATOR_ALLOC_PTR);
    assert!(std::mem::offset_of!(Mutator, alloc_limit) == crate::layout::MUTATOR_ALLOC_LIMIT);
    assert!(std::mem::offset_of!(Mutator, stack_limit) == crate::layout::MUTATOR_STACK_LIMIT);
    assert!(std::mem::offset_of!(Mutator, poll) == crate::layout::MUTATOR_POLL);
    assert!(std::mem::offset_of!(Mutator, scratch) == crate::layout::MUTATOR_SCRATCH);
    assert!(std::mem::offset_of!(Mutator, hash_seed) == crate::layout::MUTATOR_HASH_SEED);
    assert!(std::mem::offset_of!(Mutator, cur_task) == crate::layout::MUTATOR_CUR_TASK);
    assert!(std::mem::offset_of!(Mutator, tele_node) == crate::layout::MUTATOR_TELE_NODE);
    assert!(std::mem::offset_of!(Mutator, g0_sp) == crate::layout::MUTATOR_G0_SP);
    assert!(std::mem::offset_of!(Mutator, anchor) == crate::layout::MUTATOR_ANCHOR);
};

/// The top compiled frame of a stack, recorded where compiled code calls the
/// runtime.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct Anchor {
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
            cur_task: std::ptr::null_mut(),
            tele_node: 0,
            g0_sp: 0,
            anchor: Anchor::default(),
            span_end: space.end(),
            heap: Heap {
                space,
                spare: None,
                next_space: size,
                // SAFETY: the caller's contract.
                stack_maps: unsafe { StackMaps::decode(program) },
                types: program.types,
                stress,
                owned: Vec::new(),
                owned_since: 0,
                owned_trigger: OWNED_FLOOR,
            },
            sched: crate::sched::Sched::new(),
            roots: Vec::new(),
            unwind: crate::unwind::Unwinder::new(program.unwind, program.functions),
            on_worker_stack: false,
            polling: None,
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

    /// Record the top of this stack of compiled frames, given `frame`, the
    /// frame record a stub made when compiled code called it: the caller's
    /// frame pointer and the return address, with the caller's stack
    /// pointer just above them. Only a stub written in assembly knows that
    /// of its own frame. A Rust function compiled code calls is reached
    /// through a stub that records the same three words before any Rust
    /// runs: see [`crate::stubs`].
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

    /// Record the top of the running task's compiled frames, given the
    /// switch frame a stub built when compiled code called it: the frame
    /// record is inside it and the caller's stack pointer just above it.
    pub(crate) fn enter_switch_frame(&mut self, frame: usize) {
        use crate::sched::switch::{SWITCH_FRAME, SWITCH_FRAME_RECORD};
        // SAFETY: `frame` is a switch frame on the running task's stack.
        self.anchor = unsafe {
            Anchor {
                sp: frame + SWITCH_FRAME,
                pc: load(frame + SWITCH_FRAME_RECORD + WORD),
                fp: load(frame + SWITCH_FRAME_RECORD),
            }
        };
    }

    /// Where the worker's own stack is free from, when a task is running
    /// and no runtime function has moved there: see
    /// [`Self::leave_task_stack`].
    pub(crate) fn worker_stack(&self) -> Option<usize> {
        (!self.cur_task.is_null() && !self.on_worker_stack).then_some(self.g0_sp)
    }

    /// Say whether a runtime function is running on the worker's own stack
    /// in place of the task's.
    pub(crate) fn leave_task_stack(&mut self, left: bool) {
        self.on_worker_stack = left;
    }

    /// The scratch word, which compiled code and the runtime hand each
    /// other one word in around a call.
    pub(crate) fn scratch(&self) -> usize {
        self.scratch
    }

    pub(crate) fn set_scratch(&mut self, word: usize) {
        self.scratch = word;
    }

    /// The top compiled frame: the return address into it and its frame
    /// pointer, as [`Self::enter`] recorded them.
    pub(crate) fn top_frame(&self) -> (usize, usize) {
        (self.anchor.pc, self.anchor.fp)
    }

    /// Where the running task's chain of compiled frames ends.
    pub(crate) fn base_frame(&self) -> usize {
        if self.cur_task.is_null() {
            return 0;
        }
        // SAFETY: the current task is a task in use.
        unsafe { crate::sched::stack_base(self.cur_task) }
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

    /// The address of the program's type table.
    pub fn types(&self) -> usize {
        self.heap.types
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
            // The description of a type with no one type id.
            described if described & crate::json_layout::DESCRIBED as usize != 0 => true,
            crate::layout::TYPE_BOX_INT
            | crate::layout::TYPE_BOX_FLOAT
            | crate::layout::TYPE_BOX_BOOL => false,
            // SAFETY: any other id is an entry of the type table. An enum's
            // boxes hold no reference and locate the enum's names; a class
            // locates its description, and says that it is one.
            _ => unsafe {
                let info = &*((self.heap.types + type_id) as *const TypeInfo);
                !(info.kind == KIND_FIXED
                    && info.pointer_count == 0
                    && info.storage != 0
                    && info.storage & crate::layout::TYPE_DESCRIBED == 0)
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

    /// A new object that owns the Rust value at `value`, of the kind at
    /// `kind`: its `drop` is called with `value` once, by the collection
    /// that finds the object dead or when the program ends. May collect.
    pub(crate) fn allocate_owned(&mut self, value: usize, kind: &'static RustKind) -> usize {
        if self.heap.owned_since >= self.heap.owned_trigger {
            self.collect(OWNED_SIZE);
        }
        let object = self.allocate_object(TYPE_OWNED, OWNED_SIZE);
        // SAFETY: the object has two words after its header.
        unsafe {
            store(object + OWNED_VALUE, value);
            store(object + OWNED_KIND, std::ptr::from_ref(kind) as usize);
        }
        self.heap.owned.push(object);
        self.heap.owned_since += 1;
        object
    }

    /// Drop every Rust value the heap owns: the program is over, and no
    /// object is used again.
    pub fn drop_owned(&mut self) {
        for object in std::mem::take(&mut self.heap.owned) {
            // SAFETY: the list is of objects that own a value not yet
            // dropped, wherever a collection left them.
            unsafe { drop_value(object) };
        }
    }

    /// How many Rust values the heap owns, dead ones not yet dropped
    /// included.
    #[cfg(test)]
    pub(crate) fn owned(&self) -> usize {
        self.heap.owned.len()
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
        let base = self.base_frame();
        let mut copy = Copy::new(&self.heap.space, to, self.heap.types);
        // SAFETY: the stack between the anchor and `base` is the running
        // task's compiled frames, described by the stack maps; a task that
        // is not running has its top frame in its switch frame; and every
        // registered root is a live slot.
        unsafe {
            copy.stack(&self.heap.stack_maps, self.anchor, base);
            for task in self.sched.tasks() {
                let roots = crate::sched::task_roots(task, self.cur_task);
                for slot in roots.slots {
                    copy.forward(slot);
                }
                if let Some((result, [sp, pc, fp], base)) = roots.stack {
                    copy.forward(result);
                    copy.stack(&self.heap.stack_maps, Anchor { sp, pc, fp }, base);
                }
                // What the `async` natives the task is inside hold.
                crate::native::drive::each_root(task, |slot| copy.forward(slot));
            }
            for &slot in &self.roots {
                copy.forward(slot);
            }
            copy.finish();
        }
        // An owned value whose object was not copied is dropped, and one
        // whose object was is followed to where it is now. The space the
        // objects were in is still whole but for their headers.
        self.heap.owned.retain_mut(|object| {
            // SAFETY: an object in the space copied from, whose header is
            // its type id or where it was copied to.
            unsafe {
                let header = load(*object);
                if header & FORWARDED == 0 {
                    drop_value(*object);
                    return false;
                }
                *object = header & !FORWARDED;
                true
            }
        });
        self.heap.owned_since = 0;
        self.heap.owned_trigger = OWNED_FLOOR.max(self.heap.owned.len() * OWNED_PER_LIVE);
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

/// Give up the heap's hold on the Rust value the object at `object` owns.
///
/// # Safety
///
/// `object` is an owned value's object, whole but for its header, whose
/// value has not been dropped.
unsafe fn drop_value(object: usize) {
    // SAFETY: the caller's contract: the second word is the address of the
    // value's kind, which lives in the executable's data.
    unsafe {
        let kind = &*(load(object + OWNED_KIND) as *const RustKind);
        (kind.drop)(load(object + OWNED_VALUE));
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
                KIND_FIXED | KIND_ARRAY | KIND_SLICE | KIND_FUTURE | KIND_OWNED => {
                    info.size as usize
                }
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
                    // What a task threw is a reference, and so is the trace
                    // it was thrown with; what it returned is one when the
                    // future's flags say so. Until it settles both words
                    // are zero.
                    KIND_FUTURE => {
                        let settled = load(scan + FUTURE_STATE) & FUTURE_SETTLED;
                        if settled == FUTURE_ERROR {
                            self.forward(scan + FUTURE_VALUE);
                            self.forward(scan + FUTURE_TRACE);
                        } else if settled == FUTURE_READY
                            && load(scan + FUTURE_FLAGS) & FUTURE_VALUE_IS_REF != 0
                        {
                            self.forward(scan + FUTURE_VALUE);
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
        array::{array_new, array_push_slow},
        layout::{
            ARRAY_LENGTH, FIRST_CLASS_TYPE, RUNTIME_TYPES, SLICE_SIZE, TYPE_ARRAY_OF_REFS,
            TYPE_STRING_SLICE, field_offset, instance_size,
        },
        string,
    };

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
            stack_maps: crate::tests::NO_STACK_MAPS.as_ptr() as usize,
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
    fn a_copied_object_keeps_the_flags_of_its_header() {
        use crate::layout::{HEADER_PANIC, TYPE_ID_MASK};

        let mut mutator = mutator(true);
        let mut flagged = mutator.allocate_object(LINK | HEADER_PANIC, instance_size(2));
        // SAFETY: `flagged` is a fresh instance of two slots.
        unsafe {
            store(flagged + field_offset(0), 7);
            store(flagged + field_offset(1), 0);
        }
        mutator.root(&mut flagged);
        let before = flagged;
        let mut head = 0;
        mutator.root(&mut head);
        push_link(&mut mutator, &mut head, 1);
        mutator.unroot(2);
        assert_ne!(flagged, before, "every allocation collects");
        // SAFETY: `flagged` was kept current across the collection.
        let header = unsafe { load(flagged) };
        assert_eq!(header & TYPE_ID_MASK, LINK);
        assert_eq!(header & !TYPE_ID_MASK, HEADER_PANIC);
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
            let mut array = array_new(&mut mutator, TYPE_ARRAY_OF_REFS, 0);
            mutator.root(&mut array);
            for index in 0..100 {
                let text = string::from_str(&mut mutator, &format!("item {index}"));
                array_push_slow(&mut mutator, array, text);
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

    /// What a string's header caches is nothing or the hash of its text,
    /// and its flags are clear.
    #[expect(clippy::cast_possible_truncation, reason = "the high half of a header")]
    fn assert_hash_is_sound(string: usize) {
        use crate::{
            layout::{HEADER_FLAGS, STRING_HASH_MASK},
            map::{str_hash, text_hash},
        };
        // SAFETY: `string` is a live string.
        unsafe {
            let expected = text_hash(string::text(string).as_bytes());
            let header = load(string);
            assert_eq!(header & HEADER_FLAGS, 0);
            let cached = (header >> 32) as u32 & STRING_HASH_MASK;
            assert!(cached == 0 || cached == expected, "a stale cached hash");
            assert_eq!(str_hash(string), expected);
            assert_eq!((load(string) >> 32) as u32, expected);
        }
    }

    #[test]
    fn a_cached_hash_is_kept_or_dropped_but_never_wrong() {
        use crate::map::str_hash;

        // The string reached before its slices, after them, and not at all
        // with slices that keep it (60 bytes each) and that leave it
        // behind (40 of 496).
        for (rooted, repeats, length) in [
            (Some(true), 2, 60),
            (Some(false), 2, 60),
            (None, 2, 60),
            (None, 8, 40),
        ] {
            let mut mutator = mutator(true);
            let text = LETTERS.repeat(repeats);
            let mut parent = string::from_str(&mut mutator, &text);
            let mut parts = [0; 3];
            for (index, part) in parts.iter_mut().enumerate() {
                *part = slice(&mut mutator, &mut parent, index * 20, index * 20 + length);
                mutator.root(part);
            }
            // SAFETY: live strings; every one caches its hash.
            unsafe {
                str_hash(parent);
                for &part in &parts {
                    str_hash(part);
                    assert_ne!(load(part) >> 32, 0);
                }
            }
            let mut early = if rooted == Some(true) { parent } else { 0 };
            let mut late = if rooted == Some(false) { parent } else { 0 };
            mutator.root_at(std::ptr::from_mut(&mut early) as usize);
            mutator.roots.rotate_right(1);
            mutator.root(&mut late);
            mutator.allocate(WORD);
            mutator.unroot(5);
            for (index, &part) in parts.iter().enumerate() {
                // SAFETY: the parts were kept current.
                unsafe {
                    assert_eq!(string::text(part), &text[index * 20..index * 20 + length]);
                    assert_eq!(is_slice(part), length == 60);
                    if is_slice(part) {
                        // Kept with the slice.
                        assert_ne!(load(part) >> 32, 0);
                        assert_hash_is_sound(load(part + SLICE_PARENT));
                    }
                }
                assert_hash_is_sound(part);
            }
            for kept in [early, late] {
                if kept != 0 {
                    // SAFETY: kept current; a string copied as it is keeps
                    // its hash.
                    unsafe { assert_ne!(load(kept) >> 32, 0) };
                    assert_hash_is_sound(kept);
                }
            }
        }
    }

    /// A stand-in for a compiled function: the bytes are never run, only
    /// their addresses are looked up.
    static CODE: [u8; 64] = [0; 64];

    /// A mutator of a program whose one compiled function is [`CODE`],
    /// about to collect with one frame on its stack: one that called the
    /// runtime from `offset` bytes into `CODE` and holds no reference.
    /// The call is in the stack maps, with no slots, if `listed`.
    fn with_a_frame_returning_into_code(offset: usize, listed: bool) -> Mutator {
        const NAME: &[u8] = b"test.code";
        let code = CODE.as_ptr() as usize;
        // The function table: its header, one record, and the name.
        let mut functions: Vec<u8> = Vec::new();
        for word in [2_u32, 1, 0, 0] {
            functions.extend_from_slice(&word.to_le_bytes());
        }
        functions.extend_from_slice(&(code as u64).to_le_bytes());
        for word in [narrow(CODE.len()), 0, 0, 16 + 24] {
            functions.extend_from_slice(&word.to_le_bytes());
        }
        functions.extend_from_slice(&narrow(NAME.len()).to_le_bytes());
        functions.extend_from_slice(NAME);
        // The stack maps: no call holds a reference, and then the calls
        // that hold none.
        let stack_maps: Vec<usize> = if listed {
            vec![0, 1, code, 1, offset]
        } else {
            vec![0, 0]
        };
        let program = Program {
            stack_maps: stack_maps.leak().as_ptr() as usize,
            types: types().as_ptr() as usize,
            unwind: 0,
            functions: functions.leak().as_ptr() as usize,
        };
        // SAFETY: the tables are in the compiler's formats.
        let mut mutator = unsafe { Mutator::new(program, true) };
        // No task is running, so the walk ends at this one frame.
        let frame: &'static mut [usize; 2] = Box::leak(Box::new([0; 2]));
        mutator.anchor = Anchor {
            sp: frame.as_ptr() as usize,
            pc: code + offset,
            fp: usize::MAX,
        };
        mutator
    }

    #[test]
    fn a_frame_whose_call_holds_no_reference_or_is_the_runtimes_has_no_roots() {
        // A call of compiled code that is listed as holding nothing.
        with_a_frame_returning_into_code(8, true).allocate(WORD);
        // A return address outside compiled code: a frame of the runtime.
        with_a_frame_returning_into_code(CODE.len() + 64, false).allocate(WORD);
    }

    #[test]
    #[ignore = "ends its process: run by the test below"]
    fn collects_with_a_compiled_frame_that_has_no_stack_map() {
        with_a_frame_returning_into_code(8, false).allocate(WORD);
    }

    /// A return address into compiled code with no stack map is a map the
    /// compiler lost: reading it as a frame without roots would lose the
    /// frame's roots too.
    #[test]
    fn a_compiled_frame_without_a_stack_map_ends_the_program() {
        let stderr = crate::tests::in_a_process_of_its_own(
            "heap::tests::collects_with_a_compiled_frame_that_has_no_stack_map",
        );
        assert!(
            stderr.contains("has no stack map") && stderr.contains("test.code"),
            "{stderr}"
        );
    }

    /// A kind whose values are counters of how often they were dropped.
    static COUNTED: RustKind = RustKind {
        drop: count_drop,
        id: std::any::TypeId::of::<usize>,
        name: std::any::type_name::<usize>,
    };

    unsafe fn count_drop(value: usize) {
        // SAFETY: the tests give the heap addresses of leaked counters.
        unsafe { *(value as *mut usize) += 1 };
    }

    fn counters(count: usize) -> &'static [usize] {
        vec![0; count].leak()
    }

    #[test]
    fn an_owned_value_is_dropped_once_when_its_object_is_dead() {
        let mut mutator = mutator(true);
        let drops = counters(100);
        let mut kept = [0; 10];
        for slot in &mut kept {
            mutator.root(slot);
        }
        for (index, counter) in drops.iter().enumerate() {
            let object = mutator.allocate_owned(std::ptr::from_ref(counter) as usize, &COUNTED);
            if index % 10 == 0 {
                kept[index / 10] = object;
            }
        }
        // Several collections more: nothing is dropped twice, and nothing
        // alive is dropped.
        for _ in 0..3 {
            mutator.allocate(WORD);
        }
        assert_eq!(mutator.owned(), 10);
        for (index, counter) in drops.iter().enumerate() {
            assert_eq!(*counter, usize::from(index % 10 != 0), "value {index}");
        }
        for (index, object) in kept.iter().enumerate() {
            // SAFETY: kept current across the collections.
            let value = unsafe { load(object + OWNED_VALUE) };
            assert_eq!(value, std::ptr::from_ref(&drops[index * 10]) as usize);
        }
        mutator.unroot(10);
        // The program ends: what is left is dropped, once.
        mutator.drop_owned();
        mutator.drop_owned();
        assert!(drops.iter().all(|counter| *counter == 1));
    }

    #[test]
    fn owned_values_alone_bring_on_a_collection() {
        let mut mutator = mutator(false);
        let drops = counters(10 * OWNED_FLOOR);
        let space = mutator.heap.space.start;
        for counter in drops {
            mutator.allocate_owned(std::ptr::from_ref(counter) as usize, &COUNTED);
            assert!(mutator.owned() <= OWNED_FLOOR);
        }
        // The objects were far too few to fill the space.
        assert!(drops.len() * OWNED_SIZE < INITIAL_SPACE / 8);
        assert_ne!(mutator.heap.space.start, space);
        let dropped = drops.iter().filter(|counter| **counter == 1).count();
        assert_eq!(dropped + mutator.owned(), drops.len());
        mutator.drop_owned();
        assert!(drops.iter().all(|counter| *counter == 1));
    }
}
