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
/// character count (high half), and the UTF-8 text.
pub const STRING_LENGTHS: usize = HEADER;
pub const STRING_TEXT: usize = HEADER + WORD;
pub const STRING_CHAR_COUNT_SHIFT: u32 = 32;

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
pub const TYPE_STRING: usize = 8;
pub const TYPE_VALUES: usize = 24;
pub const TYPE_REFS: usize = 40;
pub const TYPE_ARRAY_OF_VALUES: usize = 56;
pub const TYPE_ARRAY_OF_REFS: usize = 72;
/// Cells holding one slot that is not a reference, one that is, and two
/// that are not.
pub const TYPE_CELL_VALUE: usize = 88;
pub const TYPE_CELL_REF: usize = 104;
pub const TYPE_CELL_PAIR: usize = 128;
/// The boxes of an `int`, a `float` and a `bool`.
pub const TYPE_BOX_INT: usize = 144;
pub const TYPE_BOX_FLOAT: usize = 160;
pub const TYPE_BOX_BOOL: usize = 176;
/// Where the types of a program's classes begin.
pub const FIRST_CLASS_TYPE: usize = 192;

/// The start of every type table, as `u32`s: eight bytes no type lives at,
/// so that no id is zero, then the fixed types.
pub const RUNTIME_TYPES: [u32; FIRST_CLASS_TYPE / 4] = [
    0,
    0,
    KIND_STRING,
    0,
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
];

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

/// Round a byte count up to whole words.
pub const fn words(bytes: usize) -> usize {
    (bytes + WORD - 1) & !(WORD - 1)
}
