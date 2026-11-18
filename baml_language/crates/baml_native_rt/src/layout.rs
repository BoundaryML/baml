//! The layout of heap objects, of types and of the mutator, as plain
//! constants.
//!
//! The compiler includes this file by path, so the offsets compiled code
//! uses inline and the ones the runtime uses cannot drift apart.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "every constant here is a small offset or size"
)]

/// Size of every slot, and the alignment of every object.
pub const WORD: usize = 8;

/// Every object starts with one header word. Its low half is the object's
/// type id, the byte offset of the type in the program's type table; bit 0
/// is the collector's. Its high half belongs to the type: zero today.
pub const HEADER: usize = WORD;
/// The bits of a header that are the type id.
pub const TYPE_ID_MASK: usize = 0xFFFF_FFF8;

/// Offset of slot `index` of a class instance.
pub const fn field_offset(index: usize) -> usize {
    HEADER + index * WORD
}

/// Size of a class instance with `slots` slots.
pub const fn instance_size(slots: usize) -> usize {
    HEADER + slots * WORD
}

/// An array is its length and the address of its first element, which is
/// zero while the array has never held one. The elements live in a separate
/// storage object so that the array can grow without moving.
pub const ARRAY_LENGTH: usize = field_offset(0);
pub const ARRAY_DATA: usize = field_offset(1);
pub const ARRAY_SIZE: usize = instance_size(2);

/// Storage is a header, a capacity, and that many 8-byte elements. An
/// array's data address points past the first two.
pub const STORAGE_CAPACITY: usize = HEADER;
pub const STORAGE_ELEMENTS: usize = HEADER + WORD;
/// Where the capacity is, from an array's data address.
pub const DATA_CAPACITY: i32 = -(WORD as i32);

/// A string is a header, one word holding the byte length (low half) and the
/// character count (high half), and then either its UTF-8 text or, for a
/// slice, where its text is: the string it is a part of and the byte of
/// that string's text it starts at. The lengths are a slice's own. What a
/// slice is a part of is never itself a slice.
pub const STRING_LENGTHS: usize = HEADER;
pub const STRING_TEXT: usize = HEADER + WORD;
pub const STRING_CHAR_COUNT_SHIFT: u32 = 32;
pub const SLICE_PARENT: usize = HEADER + WORD;
pub const SLICE_OFFSET: usize = HEADER + 2 * WORD;
pub const SLICE_SIZE: usize = HEADER + 3 * WORD;
/// The fewest bytes of text a slice is made for: a shorter part of a string
/// is copied, which takes no more room than a slice. A string with this
/// much text is itself at least as large as a slice, which is what lets
/// compiled code read a slice's words of a string it has not yet told from
/// one.
pub const SLICE_MIN_BYTES: usize = 2 * WORD + 1;
const _: () = assert!(STRING_TEXT + words(SLICE_MIN_BYTES - 1) <= SLICE_SIZE);
const _: () = assert!(STRING_TEXT + words(SLICE_MIN_BYTES) >= SLICE_SIZE);

/// A cell holds a local that a closure captures: a header and the value, one
/// slot, or two for an optional scalar.
pub const CELL_VALUE: usize = field_offset(0);

/// A function value is a closure: a header, the address of the code, and
/// what the code reads its environment from. The code takes the closure
/// itself as its first argument.
pub const CLOSURE_CODE: usize = field_offset(0);
pub const CLOSURE_CAPTURES: usize = field_offset(1);

/// Where a union or `unknown` is expected, an `int`, `float`, `bool` or enum
/// variant is a box: a header whose type says which, and the value.
pub const BOX_VALUE: usize = field_offset(0);
pub const BOX_SIZE: usize = instance_size(1);

/// The kinds of object a type describes to the collector.
pub const KIND_FIXED: u32 = 0;
pub const KIND_VALUES: u32 = 1;
pub const KIND_REFS: u32 = 2;
pub const KIND_STRING: u32 = 3;
pub const KIND_ARRAY: u32 = 4;
pub const KIND_SLICE: u32 = 5;
/// A future: its value slot holds a reference when it settled with an error,
/// or with a value its flags say is one.
pub const KIND_FUTURE: u32 = 6;

/// A type is four `u32`s: its kind; the size in bytes of an object, for a
/// fixed one; the number of reference fields; and, for an array, the type id
/// of its storage. The byte offset of each reference field follows, a `u32`
/// each.
///
/// The type of an enum's boxes has no reference fields, and its fourth
/// `u32` is the offset in the table of the enum's variant names: a `u32`
/// count, then for each variant the offset in the table and the length in
/// bytes of its name, a `u32` each.
pub const TYPE_HEADER: usize = 16;

/// The types every program has, at fixed ids at the start of its type table.
///
/// A string that holds its text and one that is a slice of another come
/// first and next to each other, so that whether an object is a string is
/// one comparison of its type id with [`STRING_TYPES_END`]: no type has a
/// lower id than the first of the two.
pub const TYPE_STRING: usize = 8;
pub const TYPE_STRING_SLICE: usize = 24;
/// Every type id below this is a string's.
pub const STRING_TYPES_END: usize = 40;
pub const TYPE_VALUES: usize = 40;
pub const TYPE_REFS: usize = 56;
pub const TYPE_ARRAY_OF_VALUES: usize = 72;
pub const TYPE_ARRAY_OF_REFS: usize = 88;
/// Cells holding one slot that is not a reference, one that is, and two
/// that are not.
pub const TYPE_CELL_VALUE: usize = 104;
pub const TYPE_CELL_REF: usize = 120;
pub const TYPE_CELL_PAIR: usize = 144;
/// The boxes of an `int`, a `float` and a `bool`.
pub const TYPE_BOX_INT: usize = 160;
pub const TYPE_BOX_FLOAT: usize = 176;
pub const TYPE_BOX_BOOL: usize = 192;
/// An error of the standard library, thrown by the runtime: one reference
/// to the text the error is written as. No type a program names is this
/// one, so only a handler that takes every error catches it.
pub const TYPE_ERROR: usize = 208;
/// A panic raised by the runtime or by compiled code's own checks, laid out
/// like [`TYPE_ERROR`]. A handler that takes every error does not take a
/// panic, so the two are told apart.
pub const TYPE_PANIC: usize = 232;
/// What a `spawn` returns and an `await` waits on: see [`FUTURE_STATE`].
pub const TYPE_FUTURE: usize = 256;
/// Where the types of a program's classes begin.
pub const FIRST_CLASS_TYPE: usize = 272;

/// The start of every type table, as `u32`s: eight bytes no type lives at,
/// so that no id is zero, then the fixed types.
pub const RUNTIME_TYPES: [u32; FIRST_CLASS_TYPE / 4] = [
    0,
    0,
    KIND_STRING,
    0,
    0,
    0,
    // The collector follows a slice's one reference by its own rule, so it
    // is not listed.
    KIND_SLICE,
    SLICE_SIZE as u32,
    0,
    0,
    KIND_VALUES,
    0,
    0,
    0,
    KIND_REFS,
    0,
    0,
    0,
    KIND_ARRAY,
    ARRAY_SIZE as u32,
    0,
    TYPE_VALUES as u32,
    KIND_ARRAY,
    ARRAY_SIZE as u32,
    0,
    TYPE_REFS as u32,
    KIND_FIXED,
    instance_size(1) as u32,
    0,
    0,
    KIND_FIXED,
    instance_size(1) as u32,
    1,
    0,
    CELL_VALUE as u32,
    0,
    KIND_FIXED,
    instance_size(2) as u32,
    0,
    0,
    KIND_FIXED,
    BOX_SIZE as u32,
    0,
    0,
    KIND_FIXED,
    BOX_SIZE as u32,
    0,
    0,
    KIND_FIXED,
    BOX_SIZE as u32,
    0,
    0,
    KIND_FIXED,
    BOX_SIZE as u32,
    1,
    0,
    BOX_VALUE as u32,
    0,
    KIND_FIXED,
    BOX_SIZE as u32,
    1,
    0,
    BOX_VALUE as u32,
    0,
    KIND_FUTURE,
    FUTURE_SIZE as u32,
    0,
    0,
];

const _: () = {
    assert!(RUNTIME_TYPES[TYPE_STRING / 4] == KIND_STRING);
    assert!(RUNTIME_TYPES[TYPE_STRING_SLICE / 4] == KIND_SLICE);
    assert!(RUNTIME_TYPES[TYPE_VALUES / 4] == KIND_VALUES);
    assert!(RUNTIME_TYPES[TYPE_REFS / 4] == KIND_REFS);
    assert!(RUNTIME_TYPES[TYPE_ARRAY_OF_REFS / 4 + 3] == TYPE_REFS as u32);
    assert!(RUNTIME_TYPES[TYPE_CELL_REF / 4 + 2] == 1);
    assert!(RUNTIME_TYPES[TYPE_BOX_BOOL / 4 + 1] == BOX_SIZE as u32);
    assert!(RUNTIME_TYPES[TYPE_ERROR / 4 + 4] == BOX_VALUE as u32);
    assert!(RUNTIME_TYPES[TYPE_PANIC / 4 + 4] == BOX_VALUE as u32);
    assert!(RUNTIME_TYPES[TYPE_FUTURE / 4] == KIND_FUTURE);
};

/// Where the mutator keeps the address of the next object and the address
/// its inline allocations must not pass.
pub const MUTATOR_ALLOC_PTR: usize = 0;
pub const MUTATOR_ALLOC_LIMIT: usize = WORD;
/// The lowest address the stack pointer may have on entry to a function
/// without calling the runtime.
pub const MUTATOR_STACK_LIMIT: usize = 2 * WORD;
/// Nonzero (32 bits) when a loop must call the runtime before its next
/// iteration.
pub const MUTATOR_POLL: usize = 3 * WORD;
/// One word passed each way through a slow path that takes and returns
/// nothing in registers: the size asked of the allocation slow path going
/// in, the address of the memory coming out.
pub const MUTATOR_SCRATCH: usize = 4 * WORD;

/// The task the worker is running, or zero on the worker's own stack.
pub const MUTATOR_CUR_TASK: usize = 5 * WORD;
/// Reserved for the telemetry node of the running task.
pub const MUTATOR_TELE_NODE: usize = 6 * WORD;
/// Where the worker's own stack resumes: the address of the switch frame it
/// left when it last switched to a task.
pub const MUTATOR_G0_SP: usize = 7 * WORD;
/// The top compiled frame at the runtime boundary: the stack pointer, the
/// return address and the frame pointer, a word each.
pub const MUTATOR_ANCHOR: usize = 8 * WORD;

/// A future is a header and four words. The state is zero or the address of
/// the first task waiting (tasks are aligned, so its low bits are clear)
/// while the task has not finished, then [`FUTURE_READY`] or
/// [`FUTURE_ERROR`], written once with the value already in place: a reader
/// loads the state first, with acquire ordering, and the value after.
pub const FUTURE_STATE: usize = field_offset(0);
/// What the task returned, as the bits of one slot, or what it threw.
pub const FUTURE_VALUE: usize = field_offset(1);
/// The task settling it, which is not a heap object, or zero.
pub const FUTURE_TASK: usize = field_offset(2);
pub const FUTURE_FLAGS: usize = field_offset(3);
pub const FUTURE_SIZE: usize = instance_size(4);
pub const FUTURE_READY: usize = 1;
pub const FUTURE_ERROR: usize = 2;
/// The bits of the state that say it is settled.
pub const FUTURE_SETTLED: usize = 3;
/// Set in a future's flags when the value its task returns is a reference.
pub const FUTURE_VALUE_IS_REF: usize = 1;
/// Set in a future's flags once an `await` has thrown its error.
pub const FUTURE_OBSERVED: usize = 2;

/// Round a byte count up to whole words.
pub const fn words(bytes: usize) -> usize {
    (bytes + WORD - 1) & !(WORD - 1)
}
