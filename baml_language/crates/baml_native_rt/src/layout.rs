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

/// Size of a machine word: a pointer, a header, a tag, a payload.
pub const WORD: usize = 8;
/// Size of a value: a tag word, then a payload word. Every slot of the heap
/// is one, every object is aligned to it and its size a multiple of it.
pub const VALUE: usize = 2 * WORD;
/// Where a value's tag and payload are.
pub const VALUE_TAG: usize = 0;
pub const VALUE_PAYLOAD: usize = WORD;

/// Every object starts with one header word, and every value with a tag
/// word in the same format:
///
/// ```text
/// bits  0..=2   the collector's: bit 0 says the rest is the address the
///               object was copied to, bit 1 is a mark it keeps only while
///               it collects. Zero in a tag
/// bits  3..=31  with the three below read as zeros, the type id: one of
///               the `TAG_` constants, or the byte offset in the program's
///               type table of the type of a class or of an enum's boxes
/// bits 32..=58  the payload of the header. A string caches its hash
///               here, zero while it has none ([`STRING_HASH_MASK`]). A
///               callable holds its call layout id here ([`call_layout`])
/// bits 59..=61  free
/// bit  62       [`HEADER_PANIC`]
/// bit  63       [`HEADER_BUSY`]
/// ```
///
/// A tag's type id is always that of the header of the object its payload
/// points at, and a callable's tag has the object's call layout id.
/// Nothing else in a tag says anything: a string's hash and the panic bit
/// are read from the object.
///
/// Whoever makes an object writes the whole word: compiled code as one
/// constant, the runtime from the type id and the flags. Compiled code
/// reads a type id as the 32-bit word at the object's address and tests a
/// flag in the 64-bit one, so neither masks the other out. The collector
/// copies the word with the object.
pub const HEADER: usize = WORD;
/// The bits of a header that are the type id.
pub const TYPE_ID_MASK: usize = 0xFFFF_FFF8;
/// Where the payload of a header is, and its bits once shifted down.
pub const HEADER_PAYLOAD_SHIFT: u32 = 32;
pub const HEADER_PAYLOAD_MASK: usize = 0x07FF_FFFF;
/// Set in the header of an instance of a class of `baml.panics`: what a
/// handler that takes every error lets pass. A string never has it.
pub const HEADER_PANIC: usize = 1 << HEADER_PANIC_BIT;
pub const HEADER_PANIC_BIT: u32 = 62;
/// The lock of a slot that several workers read and write: set in the tag
/// word of the slot while one of them is between its two words. No object
/// header has it, and no tag has it while the collector runs.
pub const HEADER_BUSY: usize = 1 << 63;

/// The call layout id in the header of a callable, or in the tag of a
/// function value.
pub const fn call_layout(header: usize) -> usize {
    (header >> HEADER_PAYLOAD_SHIFT) & HEADER_PAYLOAD_MASK
}

/// The bits of a callable's header that its tag has: the type id and the
/// call layout id.
pub const CALLABLE_TAG_MASK: usize = TYPE_ID_MASK | HEADER_PAYLOAD_MASK << HEADER_PAYLOAD_SHIFT;

/// The tags: the type ids fixed in the runtime, the same in every program.
///
/// The first five are immediates: the payload is the value itself, never
/// an address. A zeroed value is `null`, whose payload is always zero.
/// `TAG_OMITTED` is an argument left to its default, and is never stored.
pub const TAG_NULL: usize = 0;
pub const TAG_BOOL: usize = 1 << 3;
pub const TAG_INT: usize = 2 << 3;
pub const TAG_FLOAT: usize = 3 << 3;
pub const TAG_OMITTED: usize = 4 << 3;
/// Every other tag is of a pointer, and names the layout of the object
/// pointed at.
///
/// A string that holds its text and one that is a slice of another are
/// next to each other, so that whether an object is a string is one
/// comparison of its type id with [`STRING_TAGS_END`]: no object has a
/// lower id than the first of the two.
pub const TAG_STRING: usize = 5 << 3;
pub const TAG_STRING_SLICE: usize = 6 << 3;
/// Every type id of an object below this is a string's.
pub const STRING_TAGS_END: usize = TAG_INSTANCE;
pub const TAG_INSTANCE: usize = 7 << 3;
pub const TAG_ENUM_VARIANT: usize = 8 << 3;
pub const TAG_CLASS: usize = 9 << 3;
pub const TAG_ARRAY: usize = 10 << 3;
pub const TAG_ARRAY_STORAGE: usize = 11 << 3;
pub const TAG_MAP: usize = 12 << 3;
pub const TAG_MAP_ENTRIES: usize = 13 << 3;
pub const TAG_MAP_INDEX: usize = 14 << 3;
pub const TAG_CELL: usize = 15 << 3;
pub const TAG_FUTURE: usize = 16 << 3;
pub const TAG_CLOSURE: usize = 17 << 3;
pub const TAG_BOUND_METHOD: usize = 18 << 3;
pub const TAG_BIGINT: usize = 19 << 3;
pub const TAG_BYTES: usize = 20 << 3;
pub const TAG_BYTES_STORAGE: usize = 21 << 3;
pub const TAG_TUPLE: usize = 22 << 3;
pub const TAG_TYPE: usize = 23 << 3;
pub const TAG_OWNED: usize = 24 << 3;
pub const TAG_PLAIN: usize = 25 << 3;
pub const TAG_RAW_FUNCTION: usize = 26 << 3;
pub const TAG_REALIZED_FUNCTION: usize = 27 << 3;
pub const TAG_HOST_CLOSURE: usize = 28 << 3;
pub const TAG_PACKAGE: usize = 29 << 3;
pub const TAG_RUST_INSTANCE: usize = 30 << 3;
/// Where the types of a program's classes and enums begin: every type id
/// from here on is the byte offset of a type in the program's type table.
pub const FIRST_CLASS_TYPE: usize = 31 << 3;

/// Set in the tag a native that can throw returns, and in the one an entry
/// a native calls returns, when the value is what was thrown. No tag of a
/// value has it: it is one of the collector's bits, and such a tag is
/// never stored.
pub const THROWN: usize = 1;

/// Set in a slot word of a stack map when the frame slot is a value: its
/// tag at the offset the rest of the word gives, its payload a word after.
/// A slot word without it is the offset of one word, null or an address.
pub const STACK_MAP_VALUE: usize = 1;

/// Whether a tag is of a pointer.
pub const fn is_pointer(tag: usize) -> bool {
    tag & TYPE_ID_MASK > TAG_OMITTED
}

/// Whether the object with this header is called through its code address
/// ([`CLOSURE_CODE`]), and so has a call layout id.
pub const fn is_callable(header: usize) -> bool {
    matches!(
        header & TYPE_ID_MASK,
        TAG_CLOSURE
            | TAG_BOUND_METHOD
            | TAG_RAW_FUNCTION
            | TAG_REALIZED_FUNCTION
            | TAG_HOST_CLOSURE
    )
}

/// Round a byte count up to the size of an object.
pub const fn aligned(bytes: usize) -> usize {
    (bytes + VALUE - 1) & !(VALUE - 1)
}

/// A class instance is a header, a word for its class, and its slots.
pub const INSTANCE_CLASS: usize = HEADER;
pub const INSTANCE_SLOTS: usize = VALUE;

/// Offset of slot `index` of a class instance, and of a tuple.
pub const fn field_offset(index: usize) -> usize {
    INSTANCE_SLOTS + index * VALUE
}

/// Size of a class instance with `slots` slots, and of a tuple with as
/// many.
pub const fn instance_size(slots: usize) -> usize {
    INSTANCE_SLOTS + slots * VALUE
}

/// A tuple is a header, the number of its slots, and the slots.
pub const TUPLE_COUNT: usize = HEADER;

/// An array is its length, a word for its type, and the address of its
/// storage object, which is zero while the array has never held an
/// element. The elements live in a separate storage object so that the
/// array can grow without moving.
pub const ARRAY_LENGTH: usize = WORD;
pub const ARRAY_TY: usize = 2 * WORD;
pub const ARRAY_PTR: usize = 3 * WORD;
pub const ARRAY_SIZE: usize = 4 * WORD;

/// Storage is a header, a capacity, and that many values.
pub const STORAGE_CAPACITY: usize = HEADER;
pub const STORAGE_ELEMENTS: usize = VALUE;
/// The bytes of storage with room for `capacity` elements.
pub const fn storage_size(capacity: usize) -> usize {
    STORAGE_ELEMENTS + capacity * VALUE
}

/// A string is a header, one word holding the byte length (low half) and the
/// character count (high half), and then either its UTF-8 text or, for a
/// slice, where its text is: the string it is a part of and the byte of
/// that string's text it starts at. The lengths are a slice's own. What a
/// slice is a part of is never itself a slice. The bytes after the text of
/// a string, to the end of the object, are zero.
pub const STRING_LENGTHS: usize = HEADER;
pub const STRING_TEXT: usize = HEADER + WORD;
pub const STRING_CHAR_COUNT_SHIFT: u32 = 32;
pub const SLICE_PARENT: usize = HEADER + WORD;
pub const SLICE_OFFSET: usize = HEADER + 2 * WORD;
pub const SLICE_SIZE: usize = HEADER + 3 * WORD;
/// The bytes of a string of `bytes` bytes of text.
pub const fn string_size(bytes: usize) -> usize {
    aligned(STRING_TEXT + bytes)
}
/// The fewest bytes of text a slice is made for: a shorter part of a string
/// is copied, which takes no more room than a slice. A string with this
/// much text is itself at least as large as a slice, which is what lets
/// compiled code read a slice's words of a string it has not yet told from
/// one.
pub const SLICE_MIN_BYTES: usize = 2 * WORD + 1;
const _: () = assert!(string_size(SLICE_MIN_BYTES - 1) <= SLICE_SIZE);
const _: () = assert!(string_size(SLICE_MIN_BYTES) >= SLICE_SIZE);

/// A cell holds a local that a closure captures: a header, a word of
/// padding, and the value.
pub const CELL_VALUE: usize = VALUE;
pub const CELL_SIZE: usize = 2 * VALUE;

/// A function value is a callable: an object whose second word is the
/// address of its code, which takes the object itself as its first
/// argument, and whose header holds its call layout id.
///
/// A closure is a header, the code, a word for its function, the number of
/// its captures, and the captures.
pub const CLOSURE_CODE: usize = WORD;
pub const CLOSURE_FUNCTION: usize = 2 * WORD;
pub const CLOSURE_CAPTURE_COUNT: usize = 3 * WORD;
pub const CLOSURE_CAPTURES: usize = 4 * WORD;
/// The bytes of a closure with `captures` captures.
pub const fn closure_size(captures: usize) -> usize {
    CLOSURE_CAPTURES + captures * VALUE
}

/// A bound method is a header, the code, a word for its function, the
/// number of its type arguments, the receiver, and the type arguments, a
/// word each.
pub const BOUND_CODE: usize = CLOSURE_CODE;
pub const BOUND_FUNCTION: usize = 2 * WORD;
pub const BOUND_TYPE_ARG_COUNT: usize = 3 * WORD;
pub const BOUND_RECEIVER: usize = 4 * WORD;
pub const BOUND_TYPE_ARGS: usize = 6 * WORD;
/// The bytes of a bound method with `type_args` type arguments.
pub const fn bound_size(type_args: usize) -> usize {
    aligned(BOUND_TYPE_ARGS + type_args * WORD)
}

/// Where the type of a value is only known when the program runs, an enum
/// variant is a box: a header whose type says which enum, and the variant's
/// position. The compiler places one per variant in the executable's data.
pub const BOX_VALUE: usize = WORD;
pub const BOX_SIZE: usize = VALUE;

/// The kinds of the types of a program's type table.
///
/// A class: an instance is `size` bytes, a header, a word for its class
/// and slots.
pub const KIND_INSTANCE: u32 = 0;
/// An enum: the type is of its boxes.
pub const KIND_ENUM: u32 = 1;

/// Set in the fourth `u32` of a class's type; the other bits are the
/// offset of the class's description in the section of descriptions that
/// ends the type table (see `json_layout.rs`).
pub const TYPE_DESCRIBED: u32 = 1 << 31;

/// The type table starts with [`FIRST_CLASS_TYPE`] bytes no type lives at,
/// so that no type of the program has the id of a tag. A type is four
/// `u32`s: its kind; the size in bytes of an object; zero; and one more
/// that depends on the kind.
///
/// The fourth `u32` of the type of an enum's boxes is the offset in the
/// table of the enum's variant names: a `u32` count, then for each variant
/// the offset in the table and the length in bytes of its name, a `u32`
/// each.
///
/// The fourth `u32` of a class's type is [`TYPE_DESCRIBED`] and the offset
/// of the class's description in the section of descriptions: its name,
/// and the name, the place and the type of each of its fields. An uncaught
/// throw of an instance is written from it, the runtime finds the fields
/// of the instances it makes in it, and `baml.json` writes and reads
/// instances by it.
pub const TYPE_HEADER: usize = 16;

/// A `uint8array` is its length, in bytes, and the address of its storage
/// object, zero when it is empty. The length never changes.
pub const BYTES_LENGTH: usize = WORD;
pub const BYTES_PTR: usize = 2 * WORD;
pub const BYTES_SIZE: usize = 4 * WORD;
/// The storage of a `uint8array` is a header, its capacity in bytes, and
/// the bytes, packed. The bytes past the capacity, to the end of the
/// object, are zero.
pub const BYTES_STORAGE_CAPACITY: usize = HEADER;
pub const BYTES_STORAGE_DATA: usize = VALUE;
/// The bytes of the storage of `capacity` bytes.
pub const fn bytes_storage_size(capacity: usize) -> usize {
    aligned(BYTES_STORAGE_DATA + capacity)
}
/// The most bytes a `uint8array` holds: what a string's text may be.
pub const BYTES_MAX: usize = u32::MAX as usize;

/// An owned Rust value is a header and two words: the address of the
/// value, which is the raw form of an `Arc`, and the address of what the
/// runtime knows of the value's type, its destructor first. The second
/// word is also how a native checks that the value is of the type it
/// expects.
pub const OWNED_VALUE: usize = WORD;
pub const OWNED_KIND: usize = 2 * WORD;
pub const OWNED_SIZE: usize = 4 * WORD;

/// Plain Rust data is a header, the number of words of the value, the
/// address of what the runtime knows of the value's type, and the value's
/// own bytes, in whole words.
pub const PLAIN_WORDS: usize = WORD;
pub const PLAIN_KIND: usize = 2 * WORD;
pub const PLAIN_VALUE: usize = 3 * WORD;
/// The bytes of plain data of `words` words.
pub const fn plain_size(words: usize) -> usize {
    aligned(PLAIN_VALUE + words * WORD)
}

/// What `baml_rt_array_sort` orders the elements of an array as.
pub const SORT_INTS: usize = 0;
pub const SORT_FLOATS: usize = 1;
pub const SORT_STRINGS: usize = 2;
pub const SORT_BIGINTS: usize = 3;

/// A `bigint` is a header, the number of its limbs and the limbs: 64-bit
/// words, least significant first, in two's complement, as few as hold the
/// value with its sign and never none. Two that are equal are the same
/// words.
pub const BIGINT_LIMB_COUNT: usize = WORD;
pub const BIGINT_LIMBS: usize = 2 * WORD;
/// The bytes of a `bigint` of `limbs` limbs.
pub const fn bigint_size(limbs: usize) -> usize {
    aligned(BIGINT_LIMBS + limbs * WORD)
}

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

/// A map is a header and: a word each for the types of its keys and of its
/// values; the number of keys it has and of records used, dead ones
/// included, a `u32` each; its entries object; its index object, zero while
/// the entries are few enough to search without one; and the index's slot
/// count less one and a count of the times its membership has changed, a
/// `u32` each.
pub const MAP_KEY_TY: usize = WORD;
pub const MAP_VALUE_TY: usize = 2 * WORD;
pub const MAP_LEN: usize = 3 * WORD;
pub const MAP_USED: usize = 3 * WORD + 4;
pub const MAP_ENTRIES: usize = 4 * WORD;
pub const MAP_INDEX: usize = 5 * WORD;
pub const MAP_MASK: usize = 6 * WORD;
pub const MAP_EPOCH: usize = 6 * WORD + 4;
pub const MAP_SIZE: usize = 8 * WORD;

/// Entries are a header, a capacity, that many records in insertion order,
/// and then that many `u32` hashes. A record is two values, the key and
/// then what it is set to. A record never used or deleted is all zero,
/// and so is its hash.
pub const ENTRIES_CAPACITY: usize = HEADER;
pub const ENTRIES_RECORDS: usize = VALUE;
pub const ENTRY_KEY: usize = 0;
pub const ENTRY_VALUE: usize = VALUE;
pub const ENTRY_SIZE: usize = 2 * VALUE;
/// The most records searched without an index, and the fewest entries
/// have: their hashes are compared all at once.
pub const SMALL_ENTRIES: usize = 8;
/// The bytes of entries with room for `capacity` records.
pub const fn entries_size(capacity: usize) -> usize {
    ENTRIES_RECORDS + capacity * ENTRY_SIZE + aligned(capacity * 4)
}

/// An index is a header, the number of its slots, and the slots, a word
/// each: the hash of a record in the low half and its number plus one in
/// the high half.
pub const INDEX_SLOT_COUNT: usize = HEADER;
pub const INDEX_SLOTS: usize = VALUE;
/// The bytes of an index of `slots` slots, an even number.
pub const fn index_size(slots: usize) -> usize {
    aligned(INDEX_SLOTS + slots * WORD)
}

/// A hash as entries and an index hold it has this bit set, so that it is
/// neither of the two values an index slot's low half has when the slot
/// holds no entry: 0 for one never used, 1 for one whose entry was deleted.
pub const HASH_SET: u32 = 0x8000_0000;
/// The bits of a string's header's high half that cache its hash, zero
/// while it has not been computed: the payload of the header.
pub const STRING_HASH_MASK: u32 = HEADER_PAYLOAD_MASK as u32;

const _: () = {
    assert!(STRING_TAGS_END == TAG_STRING_SLICE + 8);
    assert!(TAG_RUST_INSTANCE + 8 == FIRST_CLASS_TYPE);
    assert!(TYPE_HEADER.is_multiple_of(8));
    assert!(SLICE_SIZE.is_multiple_of(VALUE));
    assert!(ARRAY_SIZE.is_multiple_of(VALUE));
    assert!(MAP_SIZE.is_multiple_of(VALUE));
    assert!(MAP_INDEX == MAP_ENTRIES + WORD);
    assert!(FUTURE_SIZE.is_multiple_of(VALUE));
    assert!(FUTURE_VALUE.is_multiple_of(VALUE));
    assert!(OWNED_SIZE.is_multiple_of(VALUE));
    assert!(BYTES_SIZE.is_multiple_of(VALUE));
    assert!(BOUND_RECEIVER.is_multiple_of(VALUE));
    // A header's payload is below the flags and above the type id.
    assert!(
        (HEADER_PAYLOAD_MASK << HEADER_PAYLOAD_SHIFT)
            & (HEADER_PANIC | HEADER_BUSY | TYPE_ID_MASK | 7)
            == 0
    );
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
/// it to do next, beside one word: return the value the native left in its
/// state, throw that value with the trace in the worker's scratch word,
/// call the compiled entry at the word, or wait for the I/O the word is the
/// address of.
pub const STEP_DONE: usize = 0;
pub const STEP_THREW: usize = 1;
pub const STEP_CALL: usize = 2;
pub const STEP_WAIT: usize = 3;
/// How many words a native passes the compiled entry it calls, two values
/// as a tag and a payload each, and where the stub finds them in the
/// native's state. A native that is done, or that threw, leaves its value
/// in the first two.
pub const NATIVE_CALL_ARGS: usize = 4;
pub const NATIVE_STATE_ARGS: usize = 0;

/// A future is a header and: its state, a value, and three words. The state
/// is zero or the address of the first task waiting (tasks are aligned, so
/// its low bits are clear) while the task has not finished, then
/// [`FUTURE_READY`] or [`FUTURE_ERROR`], written once with the value
/// already in place: a reader loads the state first, with acquire
/// ordering, and the value after.
pub const FUTURE_STATE: usize = WORD;
/// What the task returned, or what it threw. `null` until it settles.
pub const FUTURE_VALUE: usize = 2 * WORD;
/// The task settling it, which is not a heap object, or zero.
pub const FUTURE_TASK: usize = 4 * WORD;
pub const FUTURE_FLAGS: usize = 5 * WORD;
/// What the error it settled with travels with, for the `await` that throws
/// it again: the trace of the task's throw, or the context a handler in the
/// task made of that. Zero until then, and then a reference.
pub const FUTURE_TRACE: usize = 6 * WORD;
pub const FUTURE_SIZE: usize = 8 * WORD;
pub const FUTURE_READY: usize = 1;
pub const FUTURE_ERROR: usize = 2;
/// The bits of the state that say it is settled.
pub const FUTURE_SETTLED: usize = 3;
/// Set in a future's flags once an `await` has thrown its error.
pub const FUTURE_OBSERVED: usize = 1;
