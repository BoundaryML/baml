//! How the compiler describes types to the runtime, as plain constants:
//! to `baml.json`, which writes and reads values by their descriptions,
//! and to what writes an instance that is thrown and not caught.
//!
//! The compiler includes this file by path, as it does `layout.rs`.
//!
//! The descriptions are one section of the type table, past every type.
//! The `u32` at [`JSON_SECTION`] of the table is the section's offset in
//! the table, zero when the program has none. Every offset inside the
//! section is from the section's start; every field is a `u32`.
//!
//! ```text
//! +0   u32  format version (1)
//! +4   u32  offset of the directory
//! +8   u32  N, the number of directory entries
//! +12  u32  zero
//! +16  descriptions, their records and their text, in no order
//! directory: N entries of 8 bytes, sorted by type id:
//!        +0  u32  the type id of a class, or of an enum's boxes
//!        +4  u32  the offset of its description
//! ```
//!
//! The directory is how `baml.json` describes a value whose static type
//! says nothing (an `unknown`, a member of a union): by the type id in its
//! header. It holds the classes and enums `baml.json` can write and read.
//!
//! Every class has a description, which the fourth `u32` of its type
//! locates (`TYPE_DESCRIBED` of `layout.rs`). One with [`FLAG_SHOWN_ONLY`]
//! says only how an instance is written when it is thrown: `baml.json`
//! has no form for the class, or it is a generic class at some type
//! arguments, and the directory leaves it out.
//!
//! A description is [`DESCRIPTION`] bytes:
//!
//! ```text
//! +0   u32  kind, one of the `KIND_` constants
//! +4   u32  how a slot holds a value of the type, one of `REPR_`: a slot
//!           is a tag word and a payload word (`VALUE` of `layout.rs`)
//! +8   u32  a   } what they are depends on the kind
//! +12  u32  b   }
//! +16  u32  c   }
//! +20  u32  offset of the type's name, as an error message writes it
//! +24  u32  length in bytes of the name
//! +28  u32  flags
//! ```
//!
//! | kind          | a                  | b                 | c        |
//! |---------------|--------------------|-------------------|----------|
//! | `UNKNOWN`     |                    |                   |          |
//! | `ENUM`        | type id of boxes   | variant records   | count    |
//! | `CLASS`       | type id            | field records     | count    |
//! | `ARRAY`       | element            |                   |          |
//! | `MAP`         | key                | value             |          |
//! | `OPTIONAL`    | the type but null  |                   |          |
//! | `UNION`       | member list        | count             |          |
//! | `STRING_IS`   | text offset        | text length       |          |
//! | `INT_IS`      | low half           | high half         |          |
//! | `BOOL_IS`     | 0 or 1             |                   |          |
//! | `VARIANT`     | the enum           | variant index     |          |
//! | `OPAQUE`      |                    |                   |          |
//!
//! `UNKNOWN` with [`FLAG_JSON`] is the `json` alias. `OPAQUE` is a field
//! of a class shown only that nothing says how to write. An enum shown
//! only has its type id and no records. A member list is one
//! `u32` description offset per member. `OPTIONAL` is a union of `null`
//! and one other type; a `UNION` is any other, its members as declared.
//! Text is UTF-8, padded to four bytes.
//!
//! A variant record is [`VARIANT_RECORD`] bytes: the offset and length of
//! the variant's name, of the name it is written under with declared names
//! and of the one with aliased names, then flags ([`MEMBER_SKIP`]). After
//! an enum's records come the offset and length of its name with its
//! package (`user.Color`), which one message of the interpreter writes.
//!
//! A field record is [`FIELD_RECORD`] bytes, the fields in declaration
//! order:
//!
//! ```text
//! +0   u32  offset of the field's name
//! +4   u32  its length
//! +8   u32  offset of the name it is written under with declared names
//! +12  u32  its length
//! +16  u32  offset of the name with aliased names
//! +20  u32  its length
//! +24  u32  offset of the description of the field's type
//! +28  u32  byte offset of the field's slot in an instance: the address
//!           of its tag word, the payload a word after it. Every field
//!           is one slot
//! +32  u32  flags ([`MEMBER_SKIP`], [`FIELD_OMIT_NULL`],
//!           [`FIELD_MAY_BE_ABSENT`])
//! ```
//!
//! After a class's records come the offset and length of its name with
//! its package (`user.Point`), as an uncaught throw of an instance writes
//! it. Names that are the same text are the same bytes: the three of a
//! field with no alias, and a class's name where it ends its full one.
//!
//! A class with a `to_json` or a `from_json` of its own ([`FLAG_HOOKS`])
//! has one more `u32` there: the offset of two words of 64 bits each,
//! aligned as such, the addresses of the code `baml.json` calls for the
//! one and for the other, zero for the one it leaves to the language. Each
//! is an entry the compiler makes for natives to call, not the function
//! itself.

/// Where in the type table the offset of the JSON section is: the second
/// half of the eight bytes no type lives at.
pub const JSON_SECTION: usize = 4;
pub const JSON_VERSION: u32 = 1;
pub const JSON_DIRECTORY_AT: usize = 4;
pub const JSON_DIRECTORY_COUNT: usize = 8;
pub const JSON_BODY: usize = 16;

/// Set in a type id a `baml.json` native is passed when the rest is the
/// offset of a description, not a type id.
pub const DESCRIBED: u32 = 1 << 31;

pub const DESCRIPTION: usize = 32;
pub const DESCRIPTION_REPR: usize = 4;
pub const DESCRIPTION_A: usize = 8;
pub const DESCRIPTION_B: usize = 12;
pub const DESCRIPTION_C: usize = 16;
pub const DESCRIPTION_NAME: usize = 20;
pub const DESCRIPTION_FLAGS: usize = 28;

pub const KIND_UNKNOWN: u32 = 0;
pub const KIND_NULL: u32 = 1;
pub const KIND_BOOL: u32 = 2;
pub const KIND_INT: u32 = 3;
pub const KIND_FLOAT: u32 = 4;
pub const KIND_STRING: u32 = 5;
pub const KIND_ENUM: u32 = 6;
pub const KIND_CLASS: u32 = 7;
pub const KIND_ARRAY: u32 = 8;
pub const KIND_MAP: u32 = 9;
pub const KIND_OPTIONAL: u32 = 10;
pub const KIND_UNION: u32 = 11;
pub const KIND_STRING_IS: u32 = 12;
pub const KIND_INT_IS: u32 = 13;
pub const KIND_BOOL_IS: u32 = 14;
pub const KIND_VARIANT: u32 = 15;
pub const KIND_OPAQUE: u32 = 16;

/// An `int`, or the position of an enum's variant: the slot has the tag
/// of an `int`, and its payload is the value.
pub const REPR_INT: u32 = 0;
/// A `float`: the slot has the tag of one, and its payload is the bits.
pub const REPR_FLOAT: u32 = 1;
/// A `bool`: the slot has the tag of one, and its payload is zero or one.
pub const REPR_BOOL: u32 = 2;
/// A reference to an object of the type, with the tag of its object;
/// `null` is the `null` value.
pub const REPR_REF: u32 = 3;
/// A reference to an object whose header says what it is, with an `int`,
/// `float`, `bool` or enum variant in a box; `null` is the `null` value.
pub const REPR_BOXED: u32 = 4;
/// An optional `int`, `float` or `bool`, in one slot: the `null` value, or
/// the scalar with its own tag.
pub const REPR_PAIR: u32 = 5;
/// `null`: the slot holds the `null` value.
pub const REPR_NULL: u32 = 6;

/// The description is of the `json` alias.
pub const FLAG_JSON: u32 = 1;
/// The description of a class or an enum says only how a value is written
/// when it is thrown: `baml.json` neither writes nor reads one by it.
pub const FLAG_SHOWN_ONLY: u32 = 2;

/// The description of a class is followed by the entries of its own
/// `to_json` and `from_json`.
pub const FLAG_HOOKS: u32 = 4;

/// Where the offset of the two entries is, after a class's records, and
/// where each entry is from that offset.
pub const HOOKS: usize = 8;
pub const HOOK_TO_JSON: usize = 0;
pub const HOOK_FROM_JSON: usize = 8;

pub const VARIANT_RECORD: usize = 28;
pub const FIELD_RECORD: usize = 36;
pub const MEMBER_DECLARED: usize = 8;
pub const MEMBER_ALIASED: usize = 16;
pub const VARIANT_FLAGS: usize = 24;
pub const FIELD_TYPE: usize = 24;
pub const FIELD_OFFSET: usize = 28;
pub const FIELD_FLAGS: usize = 32;

/// The member is `@skip`: it has no name with aliased names.
pub const MEMBER_SKIP: u32 = 1;
/// The field is left out when it holds `null` (`@@json.omit_nulls`).
pub const FIELD_OMIT_NULL: u32 = 2;
/// The field's type admits `null`, so its key may be missing.
pub const FIELD_MAY_BE_ABSENT: u32 = 4;
