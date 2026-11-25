//! The allocator and the collector.
//!
//! # Objects
//!
//! [`crate::layout`] gives the layout of every object. A slot of an object
//! is a [`Value`]: a tag word and a payload word. The tag says whether the
//! payload is an address, and the type in an object's header says where its
//! slots are and which of its other words are addresses. Objects the
//! compiler places in the executable's data (string constants) have the
//! same layout and are never moved or freed.
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
    ARRAY_PTR, ARRAY_SIZE, BIGINT_LIMB_COUNT, BOUND_RECEIVER, BOUND_TYPE_ARG_COUNT, BYTES_PTR,
    BYTES_SIZE, BYTES_STORAGE_CAPACITY, CALLABLE_TAG_MASK, CELL_SIZE, CELL_VALUE,
    CLOSURE_CAPTURE_COUNT, CLOSURE_CAPTURES, ENTRIES_CAPACITY, ENTRIES_RECORDS, ENTRY_KEY,
    ENTRY_SIZE, ENTRY_VALUE, FIRST_CLASS_TYPE, FUTURE_SIZE, FUTURE_TRACE, FUTURE_VALUE,
    INDEX_SLOT_COUNT, INSTANCE_SLOTS, KIND_INSTANCE, MAP_ENTRIES, MAP_INDEX, MAP_SIZE, OWNED_KIND,
    OWNED_SIZE, OWNED_VALUE, PLAIN_WORDS, SLICE_OFFSET, SLICE_PARENT, SLICE_SIZE, STACK_MAP_VALUE,
    STORAGE_CAPACITY, STORAGE_ELEMENTS, STRING_LENGTHS, STRING_TEXT, TAG_ARRAY, TAG_ARRAY_STORAGE,
    TAG_BIGINT, TAG_BOOL, TAG_BOUND_METHOD, TAG_BYTES, TAG_BYTES_STORAGE, TAG_CELL, TAG_CLOSURE,
    TAG_FLOAT, TAG_FUTURE, TAG_INT, TAG_MAP, TAG_MAP_ENTRIES, TAG_MAP_INDEX, TAG_NULL, TAG_OWNED,
    TAG_PLAIN, TAG_STRING, TAG_STRING_SLICE, TAG_TUPLE, TUPLE_COUNT, TYPE_HEADER, TYPE_ID_MASK,
    VALUE, VALUE_PAYLOAD, WORD, aligned, bigint_size, bound_size, bytes_storage_size, closure_size,
    entries_size, index_size, instance_size, is_callable, is_pointer, plain_size, storage_size,
    string_size,
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

/// Set in a registered root's address when the slot is a value, which is
/// aligned as one, and not one word.
const ROOT_IS_VALUE: usize = 1;

/// What an abandoned space is filled with under stress, so that a reference
/// the collector failed to update reads as garbage.
const POISON: u8 = 0xA5;

/// The type of a class or of an enum's boxes: an entry of the program's
/// type table.
#[repr(C)]
struct TypeInfo {
    kind: u32,
    /// Bytes an object occupies, header included.
    size: u32,
    unused: u32,
    /// Where an enum's names are, or a class's description.
    extra: u32,
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

/// A slot of the heap: a tag word in the format of an object's header, and
/// a payload word the tag says how to read. An `int`, a `float` and a
/// `bool` are their bits, `null` is all zero, and anything else is the
/// address of an object whose header has the tag's type id.
///
/// [`Self::load`] and [`Self::store`] are the only code of the runtime that
/// reads or writes a slot of an object another worker may reach.
#[repr(C, align(16))]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Value {
    pub tag: usize,
    pub payload: usize,
}

const _: () = {
    assert!(size_of::<Value>() == VALUE);
    assert!(std::mem::offset_of!(Value, payload) == VALUE_PAYLOAD);
};

#[expect(
    clippy::cast_possible_truncation,
    reason = "a word is 64 bits wherever the runtime is built"
)]
impl Value {
    pub const NULL: Self = Self {
        tag: TAG_NULL,
        payload: 0,
    };

    pub const fn int(value: i64) -> Self {
        Self {
            tag: TAG_INT,
            payload: value.cast_unsigned() as usize,
        }
    }

    pub const fn float(value: f64) -> Self {
        Self {
            tag: TAG_FLOAT,
            payload: value.to_bits() as usize,
        }
    }

    pub const fn bool(value: bool) -> Self {
        Self {
            tag: TAG_BOOL,
            payload: value as usize,
        }
    }

    /// The immediate of tag `tag` whose bits are `bits`: an `int`, a
    /// `float`, a `bool`, or `null`, whose payload is zero whatever the
    /// bits.
    pub const fn immediate(tag: usize, bits: usize) -> Self {
        debug_assert!(!is_pointer(tag));
        Self {
            tag,
            payload: if tag == TAG_NULL { 0 } else { bits },
        }
    }

    /// The value that refers to the object at `object`, or `null` for
    /// zero: its tag is read from the object's header.
    ///
    /// # Safety
    ///
    /// `object` is zero or the address of a live object.
    pub unsafe fn reference(object: usize) -> Self {
        if object == 0 {
            return Self::NULL;
        }
        // SAFETY: the caller's contract: an object starts with its header.
        let header = unsafe { load(object) };
        let mask = if is_callable(header) {
            CALLABLE_TAG_MASK
        } else {
            TYPE_ID_MASK
        };
        Self {
            tag: header & mask,
            payload: object,
        }
    }

    /// What this value is once a collection has left `payload` in the root
    /// that held its payload: the collector moves the object, and makes a
    /// string of a slice, which changes the tag.
    ///
    /// # Safety
    ///
    /// When this value is a pointer, `payload` is where its object is now.
    #[must_use]
    pub unsafe fn kept(self, payload: usize) -> Self {
        if self.is_pointer() {
            // SAFETY: the caller's contract.
            unsafe { Self::reference(payload) }
        } else {
            self
        }
    }

    /// The type id of the tag.
    pub const fn id(self) -> usize {
        self.tag & TYPE_ID_MASK
    }

    pub const fn is_null(self) -> bool {
        self.id() == TAG_NULL
    }

    /// Whether the payload is the address of an object.
    pub const fn is_pointer(self) -> bool {
        is_pointer(self.tag)
    }

    /// The word compiled code holds the value as when its static type is
    /// one type or an optional reference: the payload, which is zero for
    /// `null`.
    pub const fn word(self) -> usize {
        self.payload
    }

    /// Write the slot at `slot` of an object nothing else can reach yet:
    /// one being laid out, or a future's value, which nothing reads before
    /// the future's state says it is there.
    ///
    /// # Safety
    ///
    /// `slot` is the address of a slot of such an object, and `value` a
    /// value the slot may hold.
    pub unsafe fn init(slot: usize, value: Self) {
        // SAFETY: the caller's contract.
        unsafe { *(slot as *mut Self) = value }
    }

    /// Read the slot at `slot`.
    ///
    /// # Safety
    ///
    /// `slot` is the address of a slot of a live object.
    pub unsafe fn load(slot: usize) -> Self {
        // SAFETY: the caller's contract.
        unsafe { *(slot as *const Self) }
    }

    /// Write the slot at `slot`, both words.
    ///
    /// # Safety
    ///
    /// `slot` is the address of a slot of a live object, and `value` a
    /// value the slot may hold.
    pub unsafe fn store(slot: usize, value: Self) {
        // SAFETY: the caller's contract.
        unsafe { *(slot as *mut Self) = value }
    }
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
/// that hold references across that call. A slot is one word, null or an
/// address, or with [`STACK_MAP_VALUE`] set in its offset a value: a tag
/// word and then a payload word.
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
    /// Addresses of the runtime's own slots holding references: each a
    /// word, or with [`ROOT_IS_VALUE`] set a value.
    roots: Vec<usize>,
    /// Where a throw finds its handler.
    pub(crate) unwind: crate::unwind::Unwinder,
    /// Whether a runtime function has moved to the worker's own stack.
    on_worker_stack: bool,
    /// The `async` native this worker is polling, for a panic inside it
    /// to name.
    pub(crate) polling: Option<&'static str>,
    /// The state of the `async` native that ended last on this worker, or
    /// null: its stub reads the native's value from it, so it is freed
    /// when the next one ends.
    pub(crate) spent_native: *mut crate::native::drive::Head,
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
            spent_native: std::ptr::null_mut(),
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

    /// Keep the value in `*slot` current across allocations until the
    /// matching [`Self::unroot`]: what it points at, when it is a pointer,
    /// and its tag, which changes when a slice is made a string.
    pub fn root_value(&mut self, slot: &mut Value) {
        self.root_value_at(std::ptr::from_mut(slot) as usize);
    }

    /// Like [`Self::root_value`], for a slot known by address.
    pub fn root_value_at(&mut self, slot: usize) {
        debug_assert!(slot.is_multiple_of(VALUE));
        self.roots.push(slot | ROOT_IS_VALUE);
    }

    /// Drop the last `count` roots.
    pub fn unroot(&mut self, count: usize) {
        self.roots.truncate(self.roots.len() - count);
    }

    /// The address of the program's type table.
    pub fn types(&self) -> usize {
        self.heap.types
    }

    /// The type at `type_id` of the program's type table.
    ///
    /// # Safety
    ///
    /// `type_id` is the id of a class or of an enum's boxes.
    unsafe fn info(&self, type_id: usize) -> &'static TypeInfo {
        // SAFETY: the caller's contract: the id is the offset of an entry
        // of the type table, which lives in the executable's data.
        unsafe { &*((self.heap.types + type_id) as *const TypeInfo) }
    }

    /// The name of variant `index` of the enum whose boxes have the type
    /// `enum_type`.
    pub fn variant_name(&self, enum_type: usize, index: usize) -> &'static str {
        let types = self.heap.types;
        // SAFETY: an enum's type id is an entry of the type table whose
        // fourth word locates its names, which live in the executable's
        // data.
        unsafe {
            let names = types + self.info(enum_type).extra as usize;
            let word = |position: usize| *((names + position * 4) as *const u32) as usize;
            assert!(index < word(0), "an enum value names one of its variants");
            let (start, length) = (word(1 + 2 * index), word(2 + 2 * index));
            std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                (types + start) as *const u8,
                length,
            ))
        }
    }

    /// Allocate `size` bytes (a whole number of values) and return their
    /// address, which is aligned as an object is. The memory is not
    /// initialized. May collect: every reference the caller holds must be
    /// rooted.
    pub fn allocate(&mut self, size: usize) -> usize {
        debug_assert!(size.is_multiple_of(VALUE));
        if self.heap.stress || self.alloc_ptr + size > self.span_end {
            self.collect(size);
        }
        let address = self.alloc_ptr;
        self.alloc_ptr += size;
        self.publish_limit();
        address
    }

    /// Allocate an object of `size` bytes whose header is `header`.
    pub fn allocate_object(&mut self, header: usize, size: usize) -> usize {
        let object = self.allocate(size);
        // SAFETY: `object` is `size >= WORD` fresh bytes.
        unsafe { store(object, header) };
        object
    }

    /// A new object that owns the Rust value at `value`, of the kind at
    /// `kind`: its `drop` is called with `value` once, by the collection
    /// that finds the object dead or when the program ends. May collect.
    pub(crate) fn allocate_owned(&mut self, value: usize, kind: &'static RustKind) -> usize {
        if self.heap.owned_since >= self.heap.owned_trigger {
            self.collect(OWNED_SIZE);
        }
        let object = self.allocate_object(TAG_OWNED, OWNED_SIZE);
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
    pub(crate) fn collect(&mut self, needed: usize) {
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
        let size = aligned(self.heap.next_space.max(least));
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
                    copy.forward_word(slot);
                }
                if let Some((result, [sp, pc, fp], base)) = roots.stack {
                    copy.forward_word(result);
                    copy.stack(&self.heap.stack_maps, Anchor { sp, pc, fp }, base);
                }
                // What the `async` natives the task is inside hold.
                crate::native::drive::each_root(task, |slot| copy.forward_value(slot));
            }
            for &slot in &self.roots {
                if slot & ROOT_IS_VALUE == 0 {
                    copy.forward_word(slot);
                } else {
                    copy.forward_value(slot & !ROOT_IS_VALUE);
                }
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

/// Set in an entry of the slots set aside when the slot is the payload of
/// a value, whose tag is the word before it.
const OF_VALUE: usize = 1;

/// One copying pass from a space into a new one.
struct Copy<'a> {
    from: &'a Space,
    to: Space,
    types: usize,
    /// Where the next copied object goes.
    free: usize,
    /// The slots found holding a reference to a slice of a string that had
    /// not been reached, each with [`OF_VALUE`] when it is a value's.
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

    /// The type of an instance or of a box from its header.
    ///
    /// # Safety
    ///
    /// `header` is the header of an object of a type of the program's.
    unsafe fn info(&self, header: usize) -> &'static TypeInfo {
        // SAFETY: a type id past the tags is the offset of an entry of the
        // type table, which lives in the executable's data.
        unsafe { &*((self.types + (header & TYPE_ID_MASK)) as *const TypeInfo) }
    }

    /// How many bytes the object at `object`, whose header is `header`,
    /// occupies.
    ///
    /// # Safety
    ///
    /// `object` is a live object with that header.
    unsafe fn size(&self, object: usize, header: usize) -> usize {
        // SAFETY: an object that is as long as it has something of keeps
        // the count in its second word, a string its byte length in the
        // low half of it.
        unsafe {
            let count = || load(object + WORD);
            match header & TYPE_ID_MASK {
                TAG_STRING => string_size(count() & HEADER_LOW),
                TAG_STRING_SLICE => SLICE_SIZE,
                TAG_ARRAY => ARRAY_SIZE,
                TAG_ARRAY_STORAGE => storage_size(count()),
                TAG_MAP => MAP_SIZE,
                TAG_MAP_ENTRIES => entries_size(count()),
                TAG_MAP_INDEX => index_size(count()),
                TAG_CELL => CELL_SIZE,
                TAG_FUTURE => FUTURE_SIZE,
                TAG_CLOSURE => closure_size(load(object + CLOSURE_CAPTURE_COUNT)),
                TAG_BOUND_METHOD => bound_size(load(object + BOUND_TYPE_ARG_COUNT)),
                TAG_BIGINT => bigint_size(count()),
                TAG_BYTES => BYTES_SIZE,
                TAG_BYTES_STORAGE => bytes_storage_size(count()),
                TAG_TUPLE => instance_size(count()),
                TAG_OWNED => OWNED_SIZE,
                TAG_PLAIN => plain_size(count()),
                // An enum's box is the size its type says, as an instance
                // is.
                id if id >= FIRST_CLASS_TYPE => self.info(header).size as usize,
                id => unreachable!("an object of the tag {id}, which nothing makes"),
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

    /// Make the reference in the word at `slot` point at the copy of its
    /// object, copying it first if this is the first reference to it seen.
    /// A word that is not an address in the space being collected, null
    /// included, is left as it is.
    ///
    /// # Safety
    ///
    /// `slot` is the address of a word that holds null or a reference, and
    /// stays where it is until the collection ends.
    unsafe fn forward_word(&mut self, slot: usize) {
        // SAFETY: the caller's contract.
        unsafe { self.forward(slot, 0) }
    }

    /// Make the value at `slot` point at the copy of its object, when it
    /// is a pointer.
    ///
    /// # Safety
    ///
    /// `slot` is the address of a value: a slot of a live object, of a
    /// compiled frame or of the runtime's, which stays where it is until
    /// the collection ends.
    unsafe fn forward_value(&mut self, slot: usize) {
        // SAFETY: the caller's contract.
        unsafe {
            let tag = load(slot);
            #[cfg(checks)]
            self.check(slot, tag);
            if is_pointer(tag) {
                self.forward(slot + VALUE_PAYLOAD, OF_VALUE);
            }
        }
    }

    /// End the program unless the tag of the value at `slot` is one a slot
    /// may hold while the world is stopped: not busy, and of the type id
    /// in the header of the object it points at, with its call layout when
    /// that is a callable.
    ///
    /// # Safety
    ///
    /// As [`Self::forward_value`].
    #[cfg(checks)]
    unsafe fn check(&self, slot: usize, tag: usize) {
        if tag & crate::layout::HEADER_BUSY != 0 {
            crate::sched::fatal(&format!(
                "the slot at {slot:#x} is busy at a collection: its tag is {tag:#x}"
            ));
        }
        if !is_pointer(tag) {
            return;
        }
        // SAFETY: a pointer's payload is the address of an object, whose
        // header is its own or the address of its copy.
        let header = unsafe {
            let object = load(slot + VALUE_PAYLOAD);
            match load(object) {
                copied if self.from.contains(object) && copied & FORWARDED != 0 => {
                    load(copied & !FORWARDED)
                }
                header => header,
            }
        };
        // What a tag says of its object: the type id, and a callable's call
        // layout.
        let said = if is_callable(header) {
            CALLABLE_TAG_MASK
        } else {
            TYPE_ID_MASK
        };
        if tag & said != header & said {
            crate::sched::fatal(&format!(
                "the slot at {slot:#x} has the tag {tag:#x} and points at an object whose \
                 header is {header:#x}"
            ));
        }
    }

    /// Make the reference in `*slot` point at the copy of its object. A
    /// reference to a slice of a string not reached yet is left as it is
    /// until [`Self::settle_slices`], which is told by `of_value` whether
    /// the word is the payload of a value.
    ///
    /// # Safety
    ///
    /// `slot` is the address of a word that holds null or a reference, and
    /// stays where it is until the collection ends.
    unsafe fn forward(&mut self, slot: usize, of_value: usize) {
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
            if header & TYPE_ID_MASK == TAG_STRING_SLICE {
                let parent = load(object + SLICE_PARENT);
                if self.from.contains(parent) {
                    let reached = load(parent);
                    if reached & FORWARDED == 0 {
                        self.parked.push(slot | of_value);
                        return;
                    }
                    store(object + SLICE_PARENT, reached & !FORWARDED);
                }
            }
            let size = self.size(object, header);
            store(slot, self.copy_out(object, size));
        }
    }

    /// Settle every reference set aside: by now whatever is going to reach
    /// a string directly has.
    ///
    /// Nothing here needs scanning afterwards: a string holds no
    /// reference, and a slice copied here already points at a copy.
    ///
    /// A slice that is made a string of its own changes its type id, so
    /// every value that points at one is given the tag of what it points
    /// at now. Each such value was set aside: a slice is made a string
    /// only if no reference to it was followed before.
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
            for &entry in &parked {
                let slice = load(entry & !OF_VALUE);
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
            for &entry in &parked {
                let slot = entry & !OF_VALUE;
                let settled = self.settle(load(slot));
                store(slot, settled);
                if entry & OF_VALUE != 0 {
                    store(slot - VALUE_PAYLOAD, load(settled) & TYPE_ID_MASK);
                }
            }
        }
    }

    /// The address of what the slice at `slice`, a reference to which was
    /// set aside, is copied as.
    ///
    /// # Safety
    ///
    /// As [`Self::settle_slices`], which has counted the text of every
    /// slice set aside.
    unsafe fn settle(&mut self, slice: usize) -> usize {
        // SAFETY: the caller's contract.
        unsafe {
            let header = load(slice);
            if header & FORWARDED != 0 {
                return header & !FORWARDED;
            }
            let parent = load(slice + SLICE_PARENT);
            let mut reached = load(parent);
            if reached & FORWARDED == 0 {
                let (count, size) = (reached >> COUNT_SHIFT, Self::text_size(parent));
                if count < size && count < HEADER_LOW {
                    return self.flatten(slice);
                }
                // Its slices are no smaller than it is. The count goes,
                // and so would a hash kept where it was.
                store(parent, TAG_STRING);
                reached = self.copy_out(parent, size) | FORWARDED;
            }
            store(slice, header & !COUNTED);
            store(slice + SLICE_PARENT, reached & !FORWARDED);
            self.copy_out(slice, SLICE_SIZE)
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
        unsafe { string_size(load(string + STRING_LENGTHS) & HEADER_LOW) }
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
            // The end first, for the zeros after the text.
            *((flat + size - VALUE) as *mut Value) = Value::NULL;
            store(flat, TAG_STRING);
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
                for &entry in maps.slots(pc) {
                    let entry = entry as usize;
                    if entry & STACK_MAP_VALUE == 0 {
                        self.forward_word(sp + entry);
                    } else {
                        self.forward_value(sp + (entry & !STACK_MAP_VALUE));
                    }
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

    /// Forward the `count` values from `first` on.
    ///
    /// # Safety
    ///
    /// They are slots of a copied object.
    unsafe fn values(&mut self, first: usize, count: usize) {
        for index in 0..count {
            // SAFETY: the caller's contract.
            unsafe { self.forward_value(first + index * VALUE) };
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
            // SAFETY: `scan` is the start of a copied object, whose header
            // says where its slots and its other references are.
            unsafe {
                let header = load(scan);
                let size = self.size(scan, header);
                match header & TYPE_ID_MASK {
                    TAG_ARRAY => self.forward_word(scan + ARRAY_PTR),
                    TAG_BYTES => self.forward_word(scan + BYTES_PTR),
                    // Storage is scanned to its capacity: what an array
                    // gives up it clears.
                    TAG_ARRAY_STORAGE => {
                        self.values(scan + STORAGE_ELEMENTS, load(scan + STORAGE_CAPACITY));
                    }
                    TAG_MAP => {
                        self.forward_word(scan + MAP_ENTRIES);
                        self.forward_word(scan + MAP_INDEX);
                    }
                    // A record not in use is zero: two `null`s.
                    TAG_MAP_ENTRIES => {
                        self.values(scan + ENTRIES_RECORDS, 2 * load(scan + ENTRIES_CAPACITY));
                    }
                    TAG_CELL => self.forward_value(scan + CELL_VALUE),
                    // What a task returned or threw, and the trace it was
                    // thrown with. Until it settles both are zero.
                    TAG_FUTURE => {
                        self.forward_value(scan + FUTURE_VALUE);
                        self.forward_word(scan + FUTURE_TRACE);
                    }
                    TAG_CLOSURE => {
                        self.values(scan + CLOSURE_CAPTURES, load(scan + CLOSURE_CAPTURE_COUNT));
                    }
                    TAG_BOUND_METHOD => self.forward_value(scan + BOUND_RECEIVER),
                    TAG_TUPLE => self.values(scan + INSTANCE_SLOTS, load(scan + TUPLE_COUNT)),
                    id if id >= FIRST_CLASS_TYPE && self.info(header).kind == KIND_INSTANCE => {
                        self.values(scan + INSTANCE_SLOTS, (size - INSTANCE_SLOTS) / VALUE);
                    }
                    _ => {}
                }
                scan += size;
            }
        }
        // SAFETY: the scan is done.
        unsafe { self.settle_slices() };
    }
}

const _: () = {
    // Each of these keeps a count in its second word, which is what
    // `Copy::size` reads.
    assert!(STORAGE_CAPACITY == WORD && ENTRIES_CAPACITY == WORD && INDEX_SLOT_COUNT == WORD);
    assert!(BIGINT_LIMB_COUNT == WORD && BYTES_STORAGE_CAPACITY == WORD);
    assert!(TUPLE_COUNT == WORD && PLAIN_WORDS == WORD && STRING_LENGTHS == WORD);
    // A record is two values, one after the other.
    assert!(ENTRY_KEY == 0 && ENTRY_VALUE == VALUE && ENTRY_SIZE == 2 * VALUE);
};

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
        layout::{ARRAY_LENGTH, INSTANCE_CLASS, field_offset},
        string,
    };

    /// The id of a class of two slots, an int and a reference to another
    /// instance, in [`types`].
    const LINK: usize = FIRST_CLASS_TYPE;

    /// A type table holding `LINK`.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the size of a two-slot class"
    )]
    fn types() -> &'static [u32] {
        let mut table = vec![0; FIRST_CLASS_TYPE / 4];
        table.extend([KIND_INSTANCE, instance_size(2) as u32, 0, 0]);
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
    fn push_link(mutator: &mut Mutator, head: &mut usize, value: i64) {
        let link = mutator.allocate_object(LINK, instance_size(2));
        // SAFETY: `link` is a fresh instance of two slots, and `head` was
        // kept current across the allocation.
        unsafe {
            store(link + INSTANCE_CLASS, 0);
            Value::store(link + field_offset(0), Value::int(value));
            Value::store(link + field_offset(1), Value::reference(*head));
        }
        *head = link;
    }

    fn list_values(mut link: usize) -> Vec<i64> {
        let mut values = Vec::new();
        while link != 0 {
            // SAFETY: `link` is a live instance of `LINK`.
            unsafe {
                let (value, next) = (
                    Value::load(link + field_offset(0)),
                    Value::load(link + field_offset(1)),
                );
                assert_eq!(value.tag, TAG_INT);
                assert!(next == Value::NULL || next.tag == LINK);
                values.push(value.payload.cast_signed() as i64);
                link = next.payload;
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
        use crate::layout::HEADER_PANIC;

        let mut mutator = mutator(true);
        let mut flagged = mutator.allocate_object(LINK | HEADER_PANIC, instance_size(2));
        // SAFETY: `flagged` is a fresh instance of two slots.
        unsafe {
            store(flagged + INSTANCE_CLASS, 0);
            Value::store(flagged + field_offset(0), Value::int(7));
            Value::store(flagged + field_offset(1), Value::NULL);
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
        for value in (0..).take(links) {
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
            let mut array = array_new(&mut mutator, 0);
            mutator.root(&mut array);
            for index in 0..100 {
                let text = string::from_str(&mut mutator, &format!("item {index}"));
                array_push_slow(&mut mutator, array, TAG_STRING, text);
            }
            mutator.unroot(1);
            assert_eq!(load(array + ARRAY_LENGTH), 100);
            let elements = load(array + ARRAY_PTR) + STORAGE_ELEMENTS;
            for index in 0..100 {
                let element = Value::load(elements + index * VALUE);
                assert_eq!(element.tag, TAG_STRING);
                assert_eq!(string::text(element.payload), format!("item {index}"));
            }
        }
    }

    /// A slice of bytes `from..to` of the string in `*parent`, which is all
    /// ASCII.
    fn slice(mutator: &mut Mutator, parent: &mut usize, from: usize, to: usize) -> usize {
        mutator.root(parent);
        let slice = mutator.allocate_object(TAG_STRING_SLICE, SLICE_SIZE);
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
        unsafe { load(string) & TYPE_ID_MASK == TAG_STRING_SLICE }
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
        mutator.allocate(VALUE);
        mutator.unroot(2);
        assert!(!is_slice(part));
        assert_eq!(part, again);
        // SAFETY: `part` was kept current.
        assert_eq!(unsafe { string::text(part) }, &LETTERS.repeat(4)[5..45]);
        let live = mutator.alloc_ptr - mutator.heap.space.start;
        assert_eq!(live, string_size(40) + VALUE);
    }

    /// A value's tag names what it points at: the slots that held a slice
    /// the collector made a string of say so afterwards, and a slot that
    /// holds a slice it kept still says that.
    #[test]
    fn a_value_that_pointed_at_a_slice_made_a_string_is_tagged_a_string() {
        for kept in [false, true] {
            let mut mutator = mutator(true);
            let mut parent = string::from_str(&mut mutator, &LETTERS.repeat(4));
            let mut part = slice(&mut mutator, &mut parent, 5, 45);
            mutator.root(&mut parent);
            mutator.root(&mut part);
            let mut cells = [0; 2];
            for cell in &mut cells {
                *cell = mutator.allocate_object(TAG_CELL, CELL_SIZE);
                // SAFETY: a fresh cell, and `part` was kept current.
                unsafe { Value::store(*cell + CELL_VALUE, Value::reference(part)) };
                mutator.root(cell);
            }
            mutator.unroot(4);
            for cell in &mut cells {
                mutator.root(cell);
            }
            if kept {
                mutator.root(&mut parent);
            }
            mutator.allocate(VALUE);
            mutator.unroot(if kept { 3 } else { 2 });
            for cell in cells {
                // SAFETY: the cells were kept current.
                let value = unsafe { Value::load(cell + CELL_VALUE) };
                assert_eq!(is_slice(value.payload), kept);
                let tag = if kept { TAG_STRING_SLICE } else { TAG_STRING };
                assert_eq!(value.tag, tag);
                // SAFETY: a live string.
                assert_eq!(
                    unsafe { string::text(value.payload) },
                    &LETTERS.repeat(4)[5..45]
                );
            }
        }
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
            mutator.allocate(VALUE);
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
        mutator.allocate(VALUE);
        mutator.unroot(3);
        // SAFETY: the parts were kept current.
        unsafe {
            let shared = load(parts[0] + SLICE_PARENT);
            assert_eq!(string::text(shared), text);
            assert_eq!(load(shared), TAG_STRING);
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
        mutator.allocate(VALUE);
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
        assert_eq!(live, 3 * string_size(40) + VALUE);
    }

    /// What a string's header caches is nothing or the hash of its text,
    /// and the bits above it are clear.
    #[expect(clippy::cast_possible_truncation, reason = "the high half of a header")]
    fn assert_hash_is_sound(string: usize) {
        use crate::{
            layout::{HEADER_PAYLOAD_MASK, HEADER_PAYLOAD_SHIFT, STRING_HASH_MASK},
            map::{str_hash, text_hash},
        };
        // SAFETY: `string` is a live string.
        unsafe {
            let expected = text_hash(string::text(string).as_bytes());
            let header = load(string);
            assert_eq!(header >> HEADER_PAYLOAD_SHIFT & !HEADER_PAYLOAD_MASK, 0);
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
            mutator.allocate(VALUE);
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
        with_a_frame_returning_into_code(8, true).allocate(VALUE);
        // A return address outside compiled code: a frame of the runtime.
        with_a_frame_returning_into_code(CODE.len() + 64, false).allocate(VALUE);
    }

    #[test]
    #[ignore = "ends its process: run by the test below"]
    fn collects_with_a_compiled_frame_that_has_no_stack_map() {
        with_a_frame_returning_into_code(8, false).allocate(VALUE);
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

    /// A frame slot the stack map lists as a value is a tag and a payload:
    /// the collector follows the payload when the tag is a pointer's, and
    /// gives the slot the tag of a string when it made one of the slice it
    /// pointed at. A slot listed as one word is followed as before.
    #[test]
    fn a_frame_slot_that_is_a_value_is_followed_by_its_tag() {
        let code = CODE.as_ptr() as usize;
        for kept in [false, true] {
            // One call, 8 bytes into the code, across which the frame
            // holds a word at 0 and values at 16 and at 32. No call holds
            // nothing.
            let stack_maps: Vec<usize> = vec![
                1,
                code,
                1,
                8,
                3,
                0,
                VALUE | STACK_MAP_VALUE,
                (2 * VALUE) | STACK_MAP_VALUE,
                0,
            ];
            let program = Program {
                stack_maps: stack_maps.leak().as_ptr() as usize,
                types: types().as_ptr() as usize,
                unwind: 0,
                functions: 0,
            };
            // SAFETY: the tables are in the compiler's formats.
            let mut mutator = unsafe { Mutator::new(program, true) };
            let text = LETTERS.repeat(4);
            let mut parent = string::from_str(&mut mutator, &text);
            let part = slice(&mut mutator, &mut parent, 5, 45);
            let frame: &'static mut [Value; 3] = Box::leak(Box::new([Value::NULL; 3]));
            // The word: the string the slice is of, or null.
            frame[0].tag = if kept { parent } else { 0 };
            frame[1] = Value {
                tag: TAG_STRING_SLICE,
                payload: part,
            };
            // An `int` whose bits are the address of an object that moves.
            let int = Value {
                tag: TAG_INT,
                payload: part,
            };
            frame[2] = int;
            // No task is running, so the walk ends at this one frame.
            mutator.anchor = Anchor {
                sp: frame.as_ptr() as usize,
                pc: code + 8,
                fp: usize::MAX,
            };
            mutator.allocate(VALUE);
            mutator.anchor = Anchor::default();
            assert_eq!(frame[2], int, "an immediate is not followed");
            let held = frame[1];
            assert_ne!(held.payload, part, "every allocation collects");
            assert_eq!(is_slice(held.payload), kept);
            let tag = if kept { TAG_STRING_SLICE } else { TAG_STRING };
            assert_eq!(held.tag, tag);
            // SAFETY: the frame's slots were kept current.
            unsafe {
                assert_eq!(string::text(held.payload), &text[5..45]);
                if kept {
                    assert_ne!(frame[0].tag, parent);
                    assert_eq!(load(held.payload + SLICE_PARENT), frame[0].tag);
                    assert_eq!(string::text(frame[0].tag), text);
                }
            }
        }
    }

    /// A root of the runtime's that is a value is followed by its tag, as
    /// a slot of the heap is.
    #[test]
    fn a_rooted_value_is_followed_by_its_tag() {
        let mut mutator = mutator(true);
        let mut parent = string::from_str(&mut mutator, &LETTERS.repeat(4));
        let part = slice(&mut mutator, &mut parent, 5, 45);
        let mut held = Value {
            tag: TAG_STRING_SLICE,
            payload: part,
        };
        let int = Value {
            tag: TAG_INT,
            payload: part,
        };
        let mut scalar = int;
        mutator.root_value(&mut held);
        mutator.root_value(&mut scalar);
        mutator.allocate(VALUE);
        mutator.unroot(2);
        assert_eq!(scalar, int);
        assert_ne!(held.payload, part);
        // The slice was alone, so it is a string now, and the tag says so.
        assert_eq!(held.tag, TAG_STRING);
        // SAFETY: `held` was kept current.
        assert_eq!(
            unsafe { string::text(held.payload) },
            &LETTERS.repeat(4)[5..45]
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
            mutator.allocate(VALUE);
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

    /// Storage is scanned to its capacity, not to its array's length, and
    /// a map's entries to theirs. So whatever shortens an array or a map
    /// of references clears what it gives up: each object here is dropped
    /// by the collection after the one operation that let go of it, and
    /// the ones still held are not.
    #[test]
    fn what_an_array_or_a_map_gives_up_is_collected() {
        use crate::{
            map,
            native::{Arr, ArrRef, NativeBamlArray as _, Natives, RootSlots, Scope, Slot, View},
        };

        let mut mutator = mutator(true);
        let drops = counters(8);
        let dropped = |mutator: &mut Mutator| -> Vec<usize> {
            mutator.allocate(VALUE);
            (0..drops.len())
                .filter(|index| drops[*index] == 1)
                .collect()
        };
        // SAFETY: the calls follow the contracts of compiled code, with the
        // array and the map rooted here in place of a stack map.
        unsafe {
            let mut array = array_new(&mut mutator, 0);
            mutator.root(&mut array);
            for counter in drops {
                let mut object =
                    mutator.allocate_owned(std::ptr::from_ref(counter) as usize, &COUNTED);
                // The object is held by nothing until it is in the array.
                mutator.root(&mut object);
                array_push_slow(&mut mutator, array, TAG_OWNED, object);
                mutator.unroot(1);
            }
            assert!(dropped(&mut mutator).is_empty());

            Natives::pop(ArrRef::<Slot>::from_raw(array));
            assert_eq!(dropped(&mut mutator), [7]);
            Natives::shift(ArrRef::<Slot>::from_raw(array));
            assert_eq!(dropped(&mut mutator), [0, 7]);
            // Of 1 to 6, the second: 2.
            Natives::remove_at(ArrRef::<Slot>::from_raw(array), 1);
            assert_eq!(dropped(&mut mutator), [0, 2, 7]);
            {
                // Of 1, 3, 4, 5, 6, the two from the second on: 3 and 4.
                let slots = RootSlots::new();
                let mut scope = Scope::new(&mut mutator, &slots);
                let held = scope.root_raw::<Arr<Slot>>(array);
                let nothing = scope.alloc_array::<Slot>(0);
                assert!(Natives::splice(&mut scope, held, 1, 2, nothing).is_ok());
            }
            assert_eq!(dropped(&mut mutator), [0, 2, 3, 4, 7]);
            // Of 1, 5, 6, all but the first.
            Natives::_truncate(ArrRef::<Slot>::from_raw(array), 1);
            assert_eq!(dropped(&mut mutator), [0, 2, 3, 4, 5, 6, 7]);
            Natives::clear(ArrRef::<Slot>::from_raw(array));
            assert_eq!(dropped(&mut mutator), [0, 1, 2, 3, 4, 5, 6, 7]);
            mutator.unroot(1);

            // A map from strings to objects: one key deleted, then all.
            let drops = counters(3);
            let mut held = map::new(&mut mutator, 0);
            mutator.root(&mut held);
            let mut keys = [0; 3];
            for key in &mut keys {
                mutator.root(key);
            }
            for (index, counter) in drops.iter().enumerate() {
                keys[index] = string::from_str(&mut mutator, &format!("key {index}"));
                let mut object =
                    mutator.allocate_owned(std::ptr::from_ref(counter) as usize, &COUNTED);
                mutator.root(&mut object);
                map::set(
                    &mut mutator,
                    held,
                    Value::reference(keys[index]),
                    Value::reference(object),
                );
                mutator.unroot(1);
            }
            let dropped = |mutator: &mut Mutator| -> Vec<usize> {
                mutator.allocate(VALUE);
                (0..drops.len())
                    .filter(|index| drops[*index] == 1)
                    .collect()
            };
            assert!(dropped(&mut mutator).is_empty());
            map::delete(held, Value::reference(keys[1]));
            assert_eq!(dropped(&mut mutator), [1]);
            map::clear(held);
            assert_eq!(dropped(&mut mutator), [0, 1, 2]);
            mutator.unroot(4);
        }
        mutator.drop_owned();
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
