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

/// Every object starts with one header word:
///
/// ```text
/// bits  0..=2   the collector's: bit 0 says the rest is the address the
///               object was copied to, bit 1 is a mark it keeps only while
///               it collects
/// bits  3..=31  with the three below read as zeros, the type id: the byte
///               offset of the object's type in the program's type table
/// bits 32..=59  free in every object: an identity hash or a lock word
///               later, a string's cached hash. The collector counts in a
///               string's while it collects and leaves them zero
/// bits 60..=63  flags, in every object; bit 63 is [`HEADER_PANIC`]
/// ```
///
/// Whoever makes an object writes the whole word: compiled code as one
/// constant, the runtime from the type id and the flags. Compiled code
/// reads a type id as the 32-bit word at the object's address and tests a
/// flag in the 64-bit one, so neither masks the other out. The collector
/// copies the word with the object.
pub const HEADER: usize = WORD;
/// The bits of a header that are the type id.
pub const TYPE_ID_MASK: usize = 0xFFFF_FFF8;
/// Set in the header of an instance of a class of `baml.panics`: what a
/// handler that takes every error lets pass. A string never has it.
pub const HEADER_PANIC: usize = 1 << HEADER_PANIC_BIT;
pub const HEADER_PANIC_BIT: u32 = 63;
/// The flags of a header.
pub const HEADER_FLAGS: usize = 0xF << 60;

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

/// Set in the fourth `u32` of a type that has a description; the other
/// bits are the description's offset in the type table.
pub const TYPE_DESCRIBED: u32 = 1 << 31;
/// The size of a description before its field records, and of one record.
pub const DESCRIPTION_HEADER: usize = 12;
pub const DESCRIPTION_FIELD: usize = 20;

/// What a field of a described class holds: an `int`, a `float`, a `bool`,
/// a variant of an enum, or a reference (zero for `null`) to an object
/// whose header says what it is. The optional scalars take two slots: the
/// value's bits, then whether there is one. Anything else is opaque.
pub const FIELD_INT: u32 = 0;
pub const FIELD_FLOAT: u32 = 1;
pub const FIELD_BOOL: u32 = 2;
pub const FIELD_ENUM: u32 = 3;
pub const FIELD_REF: u32 = 4;
/// Added to the kind of a scalar for the optional of it.
pub const FIELD_OPTIONAL: u32 = 8;
pub const FIELD_OPAQUE: u32 = 16;

/// A type is four `u32`s: its kind; the size in bytes of an object, for a
/// fixed one; the number of reference fields; and one more that depends on
/// the type. The byte offset of each reference field follows, a `u32` each.
///
/// The fourth `u32` of an array's type is the type id of its storage.
///
/// The type of an enum's boxes has no reference fields, and its fourth
/// `u32` is the offset in the table of the enum's variant names: a `u32`
/// count, then for each variant the offset in the table and the length in
/// bytes of its name, a `u32` each.
///
/// The fourth `u32` of a class's type is [`TYPE_DESCRIBED`] and the offset
/// in the table of the class's description, which is past every type:
///
/// ```text
/// +0   u32  offset in the table of the class's name, as an uncaught throw
///           of an instance writes it
/// +4   u32  length in bytes of the name
/// +8   u32  N, the number of fields
/// +12  N field records of 20 bytes, in declaration order:
///        +0   u32  offset in the table of the field's name
///        +4   u32  length in bytes of the name
///        +8   u32  byte offset of the field in an instance
///        +12  u32  what the field holds, one of the `FIELD_` constants
///        +16  u32  for an enum, the type id of the enum's boxes
/// ```
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
/// Where the types of a program's classes begin.
pub const FIRST_CLASS_TYPE: usize = 208;

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
];

const _: () = {
    assert!(RUNTIME_TYPES[TYPE_STRING / 4] == KIND_STRING);
    assert!(RUNTIME_TYPES[TYPE_STRING_SLICE / 4] == KIND_SLICE);
    assert!(RUNTIME_TYPES[TYPE_VALUES / 4] == KIND_VALUES);
    assert!(RUNTIME_TYPES[TYPE_REFS / 4] == KIND_REFS);
    assert!(RUNTIME_TYPES[TYPE_ARRAY_OF_REFS / 4 + 3] == TYPE_REFS as u32);
    assert!(RUNTIME_TYPES[TYPE_CELL_REF / 4 + 2] == 1);
    assert!(RUNTIME_TYPES[TYPE_BOX_BOOL / 4 + 1] == BOX_SIZE as u32);
    assert!(RUNTIME_TYPES[TYPE_BOX_INT / 4 + 1] == BOX_SIZE as u32);
    assert!(TYPE_BOX_BOOL + TYPE_HEADER == FIRST_CLASS_TYPE);
};

/// The classes of the standard library the runtime makes instances of: the
/// panics, which are every class of `baml.panics` in declaration order, and
/// then the errors its natives throw. Each is the namespace under `baml`
/// and the class's name. The compiler emits the type id of each, in this
/// order, as the `u32`s of its well-known table, after a `u32` count; the
/// description of the type says where the fields are.
pub const WELL_KNOWN: [(&str, &str); 18] = [
    ("panics", "DivisionByZero"),
    ("panics", "IndexOutOfBounds"),
    ("panics", "InvalidFieldAccess"),
    ("panics", "MapKeyNotFound"),
    ("panics", "StackOverflow"),
    ("panics", "AssertionFailed"),
    ("panics", "Unreachable"),
    ("panics", "Cancelled"),
    ("panics", "UserPanic"),
    ("panics", "Exit"),
    ("panics", "AllocFailure"),
    ("panics", "HostUnavailable"),
    ("panics", "NegativeBitShift"),
    ("panics", "SdkPanic"),
    ("panics", "HostContractViolation"),
    ("panics", "IntegerOverflow"),
    ("errors", "InvalidArgument"),
    ("errors", "ParseError"),
];
/// How many of [`WELL_KNOWN`], from the first, are panics.
pub const WELL_KNOWN_PANICS: usize = 16;
pub const WELL_KNOWN_DIVISION_BY_ZERO: usize = 0;
pub const WELL_KNOWN_INDEX_OUT_OF_BOUNDS: usize = 1;
pub const WELL_KNOWN_NEGATIVE_BIT_SHIFT: usize = 12;
pub const WELL_KNOWN_INVALID_ARGUMENT: usize = 16;
pub const WELL_KNOWN_PARSE_ERROR: usize = 17;

/// What a handler of the unwind table takes, and what it does with what it
/// takes. A handler lands with an error when it takes every error, when it
/// takes every panic and the error is one, when it takes every error that
/// is not a panic and the error is not one, when it takes `null` and the
/// error is that, or when the error's type id is in its list.
pub const HANDLER_TAKES_ALL: u32 = 1;
pub const HANDLER_TAKES_PANICS: u32 = 2;
pub const HANDLER_TAKES_OTHERS: u32 = 4;
pub const HANDLER_TAKES_NULL: u32 = 8;
/// The handler reads the context it lands with.
pub const HANDLER_BINDS_CONTEXT: u32 = 16;
/// The handler can throw what it landed with again, with its context.
pub const HANDLER_MAY_RETHROW: u32 = 32;

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
