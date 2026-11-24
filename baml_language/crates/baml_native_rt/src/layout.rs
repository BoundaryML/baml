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
///               later. A string caches its hash here, zero while it has
///               none ([`STRING_HASH_MASK`]). The collector counts in the
///               bits of a string only slices reach, and such a string and
///               a slice made a string of its own come out with them zero
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
/// Set in the header of an array whose elements are `float`s, and of a map
/// whose values are: the words alone do not say so, and what compares two
/// values of no known type reads it. Whoever makes one sets it.
pub const HEADER_FLOATS: usize = 1 << 60;
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
pub const KIND_ENTRIES: u32 = 6;
/// A future: its value slot holds a reference when it settled with an error,
/// or with a value its flags say is one, and its trace slot when it settled
/// with an error.
pub const KIND_FUTURE: u32 = 7;
/// A Rust value the object owns: see [`OWNED_VALUE`]. It holds no
/// reference, and the collector drops the value of one it does not copy.
pub const KIND_OWNED: u32 = 8;

/// Set in the fourth `u32` of a class's type; the other bits are the
/// offset of the class's description in the section of descriptions that
/// ends the type table (see `json_layout.rs`).
pub const TYPE_DESCRIBED: u32 = 1 << 31;

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
/// of the class's description in the section of descriptions: its name,
/// and the name, the place and the type of each of its fields. An uncaught
/// throw of an instance is written from it, the runtime finds the fields
/// of the instances it makes in it, and `baml.json` writes and reads
/// instances by it.
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
/// A map: see [`MAP_SIZE`].
pub const TYPE_MAP: usize = 208;
/// The entries of a map, by which words of a record are references:
/// neither, the key, the value, both. The second `u32` of such a type is
/// the size of a record, and its reference offsets are within one.
pub const TYPE_ENTRIES: usize = 232;
pub const TYPE_ENTRIES_KEY_REF: usize = 248;
pub const TYPE_ENTRIES_VALUE_REF: usize = 272;
pub const TYPE_ENTRIES_REFS: usize = 296;
/// What a `spawn` returns and an `await` waits on: see [`FUTURE_STATE`].
pub const TYPE_FUTURE: usize = 320;
/// A `bigint`: see [`BIGINT_LIMBS`]. The collector copies it as it does an
/// array's storage of values: a count of words and that many words, none a
/// reference.
pub const TYPE_BIGINT: usize = 336;
/// A Rust value owned by the heap: what a `$rust_type` field refers to
/// when the value has a destructor. See [`OWNED_VALUE`].
pub const TYPE_OWNED: usize = 352;
/// Plain Rust data kept in the heap: what a `$rust_type` field refers to
/// when the value is `Copy`. See [`PLAIN_KIND`]. The collector copies it
/// as it does an array's storage of values.
pub const TYPE_PLAIN: usize = 368;
/// A `uint8array`: see [`BYTES_LENGTH`]. The collector copies it as it
/// does an array, whose storage it finds from the data address.
pub const TYPE_BYTES: usize = 384;
/// Where the types of a program's classes begin.
pub const FIRST_CLASS_TYPE: usize = 400;

/// A `uint8array` is laid out as an array is: its length, in bytes, and the
/// address of its first byte, zero when it is empty. The length never
/// changes. The bytes are packed in a storage object of values, whose
/// capacity is in words as any storage's is: the length rounded up to a
/// whole word, which is what the collector sizes the storage by. The bytes
/// of the last word past the length are zero.
pub const BYTES_LENGTH: usize = ARRAY_LENGTH;
pub const BYTES_DATA: usize = ARRAY_DATA;
pub const BYTES_SIZE: usize = ARRAY_SIZE;
/// The most bytes a `uint8array` holds: what a string's text may be.
pub const BYTES_MAX: usize = u32::MAX as usize;

/// An owned Rust value is a header and two words: the address of the
/// value, which is the raw form of an `Arc`, and the address of what the
/// runtime knows of the value's type, its destructor first. The second
/// word is also how a native checks that the value is of the type it
/// expects.
pub const OWNED_VALUE: usize = field_offset(0);
pub const OWNED_KIND: usize = field_offset(1);
pub const OWNED_SIZE: usize = instance_size(2);

/// Plain Rust data is a header, the number of words that follow, the
/// address of what the runtime knows of the value's type, and the value's
/// own bytes, in whole words.
pub const PLAIN_WORDS: usize = STORAGE_CAPACITY;
pub const PLAIN_KIND: usize = STORAGE_ELEMENTS;
pub const PLAIN_VALUE: usize = STORAGE_ELEMENTS + WORD;

/// What `baml_rt_array_sort` orders the elements of an array as.
pub const SORT_INTS: usize = 0;
pub const SORT_FLOATS: usize = 1;
pub const SORT_STRINGS: usize = 2;
pub const SORT_BIGINTS: usize = 3;

/// A `bigint` is a header, the number of its limbs and the limbs: 64-bit
/// words, least significant first, in two's complement, as few as hold the
/// value with its sign and never none. Two that are equal are the same
/// words.
pub const BIGINT_LIMB_COUNT: usize = STORAGE_CAPACITY;
pub const BIGINT_LIMBS: usize = STORAGE_ELEMENTS;

/// The operators of `baml_rt_bigint_binary`, in the low byte of its
/// `op`.
pub const BIGINT_OP_ADD: usize = 0;
pub const BIGINT_OP_SUB: usize = 1;
pub const BIGINT_OP_MUL: usize = 2;
pub const BIGINT_OP_DIV: usize = 3;
pub const BIGINT_OP_REM: usize = 4;
pub const BIGINT_OP_AND: usize = 5;
pub const BIGINT_OP_OR: usize = 6;
pub const BIGINT_OP_XOR: usize = 7;
pub const BIGINT_OP_SHL: usize = 8;
pub const BIGINT_OP_SHR: usize = 9;
/// Set in an `op` whose left operand is an `int`, not a `bigint` object.
pub const BIGINT_LEFT_IS_INT: usize = 1 << 8;
/// Set in an `op` whose right operand is an `int`.
pub const BIGINT_RIGHT_IS_INT: usize = 1 << 9;

/// A map is a header, whose high half is kept for a lock word, and four
/// words: the number of keys it has and of records used, dead ones
/// included, a `u32` each; its entries object; its index object, zero while
/// the entries are few enough to search without one; and the index's slot
/// count less one and a count of the times its membership has changed, a
/// `u32` each.
pub const MAP_LEN: usize = HEADER;
pub const MAP_USED: usize = HEADER + 4;
pub const MAP_ENTRIES: usize = 2 * WORD;
pub const MAP_INDEX: usize = 3 * WORD;
pub const MAP_MASK: usize = 4 * WORD;
pub const MAP_EPOCH: usize = 4 * WORD + 4;
pub const MAP_SIZE: usize = 5 * WORD;

/// Entries are a header, a capacity, that many records of a key and a
/// value in insertion order, and then that many `u32` hashes. A record
/// never used or deleted is all zero, and so is its hash. Record `n` is at
/// `(n + 1) * ENTRY_SIZE`.
pub const ENTRIES_CAPACITY: usize = HEADER;
pub const ENTRIES_RECORDS: usize = 2 * WORD;
pub const ENTRY_VALUE: usize = WORD;
pub const ENTRY_SIZE: usize = 2 * WORD;
/// The most records searched without an index, and the fewest entries
/// have: their hashes are compared all at once.
pub const SMALL_ENTRIES: usize = 8;
/// The bytes of entries with room for `capacity` records.
pub const fn entries_size(capacity: usize) -> usize {
    ENTRIES_RECORDS + capacity * ENTRY_SIZE + words(capacity * 4)
}

/// A hash as entries and an index hold it has this bit set, so that it is
/// neither of the two values an index slot's low half has when the slot
/// holds no entry: 0 for one never used, 1 for one whose entry was deleted.
pub const HASH_SET: u32 = 0x8000_0000;
/// The bits of a string's header's high half that cache its hash, zero
/// while it has not been computed. The four above them are flags.
pub const STRING_HASH_MASK: u32 = 0x0FFF_FFFF;

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
    MAP_SIZE as u32,
    2,
    0,
    MAP_ENTRIES as u32,
    MAP_INDEX as u32,
    KIND_ENTRIES,
    ENTRY_SIZE as u32,
    0,
    0,
    KIND_ENTRIES,
    ENTRY_SIZE as u32,
    1,
    0,
    0,
    0,
    KIND_ENTRIES,
    ENTRY_SIZE as u32,
    1,
    0,
    ENTRY_VALUE as u32,
    0,
    KIND_ENTRIES,
    ENTRY_SIZE as u32,
    2,
    0,
    0,
    ENTRY_VALUE as u32,
    KIND_FUTURE,
    FUTURE_SIZE as u32,
    0,
    0,
    KIND_VALUES,
    0,
    0,
    0,
    KIND_OWNED,
    OWNED_SIZE as u32,
    0,
    0,
    KIND_VALUES,
    0,
    0,
    0,
    KIND_ARRAY,
    BYTES_SIZE as u32,
    0,
    TYPE_VALUES as u32,
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
    assert!(TYPE_BOX_BOOL + TYPE_HEADER == TYPE_MAP);
    assert!(RUNTIME_TYPES[TYPE_MAP / 4 + 1] == MAP_SIZE as u32);
    assert!(RUNTIME_TYPES[TYPE_MAP / 4 + 5] == MAP_INDEX as u32);
    assert!(RUNTIME_TYPES[TYPE_ENTRIES / 4] == KIND_ENTRIES);
    assert!(RUNTIME_TYPES[TYPE_ENTRIES_KEY_REF / 4 + 2] == 1);
    assert!(RUNTIME_TYPES[TYPE_ENTRIES_VALUE_REF / 4 + 4] == ENTRY_VALUE as u32);
    assert!(RUNTIME_TYPES[TYPE_ENTRIES_REFS / 4 + 5] == ENTRY_VALUE as u32);
    assert!(TYPE_ENTRIES_REFS + TYPE_HEADER + 2 * 4 == TYPE_FUTURE);
    assert!(RUNTIME_TYPES[TYPE_FUTURE / 4] == KIND_FUTURE);
    assert!(RUNTIME_TYPES[TYPE_FUTURE / 4 + 1] == FUTURE_SIZE as u32);
    assert!(TYPE_FUTURE + TYPE_HEADER == TYPE_BIGINT);
    assert!(RUNTIME_TYPES[TYPE_BIGINT / 4] == KIND_VALUES);
    assert!(TYPE_BIGINT + TYPE_HEADER == TYPE_OWNED);
    assert!(RUNTIME_TYPES[TYPE_OWNED / 4] == KIND_OWNED);
    assert!(RUNTIME_TYPES[TYPE_OWNED / 4 + 1] == OWNED_SIZE as u32);
    assert!(RUNTIME_TYPES[TYPE_PLAIN / 4] == KIND_VALUES);
    assert!(TYPE_PLAIN + TYPE_HEADER == TYPE_BYTES);
    assert!(RUNTIME_TYPES[TYPE_BYTES / 4] == KIND_ARRAY);
    assert!(RUNTIME_TYPES[TYPE_BYTES / 4 + 3] == TYPE_VALUES as u32);
    assert!(TYPE_BYTES + TYPE_HEADER == FIRST_CLASS_TYPE);
    // A string's cached hash is in the bits every object has free, below
    // the flags.
    assert!((STRING_HASH_MASK as usize) << 32 & (HEADER_FLAGS | TYPE_ID_MASK | 7) == 0);
};

/// The classes of the standard library the runtime makes instances of: the
/// panics, which are every class of `baml.panics` in declaration order, and
/// then the errors its natives throw and the classes of an error's context.
/// The runtime gives the fields of one it makes in declaration order.
/// Each is the namespace under `baml`
/// and the class's name. The compiler emits the type id of each, in this
/// order, as the `u32`s of its well-known table, after a `u32` count; the
/// description of the type says where the fields are.
pub const WELL_KNOWN: [(&str, &str); 25] = [
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
    ("errors", "InvalidArgument"),
    ("errors", "ParseError"),
    ("errors", "Context"),
    ("errors", "StackTrace"),
    ("errors", "StackFrame"),
    ("json", "ParseError"),
    ("json", "DecodeError"),
    ("json", "SerializationError"),
    ("json", "PathError"),
    ("errors", "Io"),
];
/// The classes of the standard library a native builds an instance of and
/// has no other way to name: not a type argument of the call, not the
/// class of an object it was passed, not the type of a field of a class it
/// already has. Each is the namespace under `baml` and the class's name.
///
/// The compiler writes the type id of each after those of [`WELL_KNOWN`],
/// in this order, and writes zero for a class that no native the program
/// calls names in its declaration, which is then not described either.
pub const NAMED: [(&str, &str); 35] = [
    ("regex", "Regex"),
    ("regex", "Match"),
    ("regex", "Error"),
    ("hash", "DefaultHasher"),
    ("http", "Response"),
    ("errors", "Timeout"),
    ("crypto", "Sha256"),
    ("crypto", "Aes128GcmSiv"),
    ("crypto", "Aes256GcmSiv"),
    ("crypto", "ChaCha20Poly1305"),
    ("crypto", "XChaCha20Poly1305"),
    ("crypto", "DecryptionFailure"),
    ("fs", "File"),
    ("fs", "DirEntry"),
    ("fs", "Metadata"),
    ("fs", "Lock"),
    ("net", "TcpStream"),
    ("net", "TcpListener"),
    ("net", "UdpSocket"),
    ("net", "Datagram"),
    ("http", "SseStream"),
    ("sys", "HostInfo"),
    ("sys", "ProcessOutput"),
    ("sync", "Semaphore"),
    ("sync", "Permit"),
    ("time", "Instant"),
    ("time", "PlainDate"),
    ("time", "PlainDateTime"),
    ("time", "PlainTime"),
    ("time", "TimeZoneOffset"),
    ("time", "ZonedDateTime"),
    ("random", "Xoshiro256PlusPlus"),
    ("random", "ChaCha20"),
    ("glob", "Glob"),
    ("yaml", "ParseError"),
];
pub const NAMED_REGEX: usize = 0;
pub const NAMED_REGEX_MATCH: usize = 1;
pub const NAMED_REGEX_ERROR: usize = 2;
pub const NAMED_DEFAULT_HASHER: usize = 3;
pub const NAMED_HTTP_RESPONSE: usize = 4;
pub const NAMED_TIMEOUT: usize = 5;
pub const NAMED_SHA256: usize = 6;
pub const NAMED_AES_128_GCM_SIV: usize = 7;
pub const NAMED_AES_256_GCM_SIV: usize = 8;
pub const NAMED_CHACHA20_POLY1305: usize = 9;
pub const NAMED_XCHACHA20_POLY1305: usize = 10;
pub const NAMED_DECRYPTION_FAILURE: usize = 11;
pub const NAMED_FS_FILE: usize = 12;
pub const NAMED_FS_DIR_ENTRY: usize = 13;
pub const NAMED_FS_METADATA: usize = 14;
pub const NAMED_FS_LOCK: usize = 15;
pub const NAMED_NET_TCP_STREAM: usize = 16;
pub const NAMED_NET_TCP_LISTENER: usize = 17;
pub const NAMED_NET_UDP_SOCKET: usize = 18;
pub const NAMED_NET_DATAGRAM: usize = 19;
pub const NAMED_HTTP_SSE_STREAM: usize = 20;
pub const NAMED_SYS_HOST_INFO: usize = 21;
pub const NAMED_SYS_PROCESS_OUTPUT: usize = 22;
pub const NAMED_SYNC_SEMAPHORE: usize = 23;
pub const NAMED_SYNC_PERMIT: usize = 24;
pub const NAMED_TIME_INSTANT: usize = 25;
pub const NAMED_TIME_PLAIN_DATE: usize = 26;
pub const NAMED_TIME_PLAIN_DATE_TIME: usize = 27;
pub const NAMED_TIME_PLAIN_TIME: usize = 28;
pub const NAMED_TIME_ZONE_OFFSET: usize = 29;
pub const NAMED_TIME_ZONED_DATE_TIME: usize = 30;
pub const NAMED_RANDOM_XOSHIRO: usize = 31;
pub const NAMED_RANDOM_CHACHA20: usize = 32;
pub const NAMED_GLOB: usize = 33;
pub const NAMED_YAML_PARSE_ERROR: usize = 34;

/// How many of [`WELL_KNOWN`], from the first, are panics.
pub const WELL_KNOWN_PANICS: usize = 15;
pub const WELL_KNOWN_DIVISION_BY_ZERO: usize = 0;
pub const WELL_KNOWN_INDEX_OUT_OF_BOUNDS: usize = 1;
pub const WELL_KNOWN_MAP_KEY_NOT_FOUND: usize = 3;
pub const WELL_KNOWN_STACK_OVERFLOW: usize = 4;
pub const WELL_KNOWN_USER_PANIC: usize = 8;
pub const WELL_KNOWN_EXIT: usize = 9;
pub const WELL_KNOWN_ALLOC_FAILURE: usize = 10;
pub const WELL_KNOWN_NEGATIVE_BIT_SHIFT: usize = 12;
pub const WELL_KNOWN_INVALID_ARGUMENT: usize = 15;
pub const WELL_KNOWN_PARSE_ERROR: usize = 16;
pub const WELL_KNOWN_CONTEXT: usize = 17;
pub const WELL_KNOWN_STACK_TRACE: usize = 18;
pub const WELL_KNOWN_STACK_FRAME: usize = 19;
pub const WELL_KNOWN_JSON_PARSE_ERROR: usize = 20;
pub const WELL_KNOWN_JSON_DECODE_ERROR: usize = 21;
pub const WELL_KNOWN_JSON_SERIALIZATION_ERROR: usize = 22;
pub const WELL_KNOWN_JSON_PATH_ERROR: usize = 23;
pub const WELL_KNOWN_IO: usize = 24;

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
/// What this process's hashes are mixed with, where a map probe reads it.
pub const MUTATOR_HASH_SEED: usize = 5 * WORD;

/// The task the worker is running, or zero on the worker's own stack.
pub const MUTATOR_CUR_TASK: usize = 6 * WORD;
/// Reserved for the telemetry node of the running task.
pub const MUTATOR_TELE_NODE: usize = 7 * WORD;
/// Where the worker's own stack resumes: the address of the switch frame it
/// left when it last switched to a task.
pub const MUTATOR_G0_SP: usize = 8 * WORD;
/// The top compiled frame at the runtime boundary: the stack pointer, the
/// return address and the frame pointer, a word each.
pub const MUTATOR_ANCHOR: usize = 9 * WORD;

// The fields compiled code and the assembly stubs reach through the pinned
// register are a word each, one after the other, and none shares its word.
const _: () = {
    let fields = [
        MUTATOR_ALLOC_PTR,
        MUTATOR_ALLOC_LIMIT,
        MUTATOR_STACK_LIMIT,
        MUTATOR_POLL,
        MUTATOR_SCRATCH,
        MUTATOR_HASH_SEED,
        MUTATOR_CUR_TASK,
        MUTATOR_TELE_NODE,
        MUTATOR_G0_SP,
        MUTATOR_ANCHOR,
    ];
    let mut index = 0;
    while index < fields.len() {
        assert!(fields[index] == index * WORD);
        index += 1;
    }
};

/// What the poll of an `async` native tells the compiled stub that drives
/// it to do next, beside one word: return the word, throw the word with
/// the trace in the worker's scratch word, call the compiled entry at the
/// word, or wait for the I/O the word is the address of.
pub const STEP_DONE: usize = 0;
pub const STEP_THREW: usize = 1;
pub const STEP_CALL: usize = 2;
pub const STEP_WAIT: usize = 3;
/// How many arguments a native passes the compiled entry it calls, and
/// where the stub finds them in the native's state.
pub const NATIVE_CALL_ARGS: usize = 4;
pub const NATIVE_STATE_ARGS: usize = 0;

/// A future is a header and five words. The state is zero or the address of
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
/// What the error it settled with travels with, for the `await` that throws
/// it again: the trace of the task's throw, or the context a handler in the
/// task made of that. Zero until then.
pub const FUTURE_TRACE: usize = field_offset(4);
pub const FUTURE_SIZE: usize = instance_size(5);
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
