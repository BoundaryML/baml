//! How the compiler describes types to `baml.json`, as plain constants.
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
//! The directory is how a value whose static type says nothing (an
//! `unknown`, a member of a union) is described: by the type id in its
//! header. It holds the classes and enums some description names.
//!
//! A description is [`DESCRIPTION`] bytes:
//!
//! ```text
//! +0   u32  kind, one of the `KIND_` constants
//! +4   u32  how a slot holds a value of the type, one of `REPR_`
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
//!
//! `UNKNOWN` with [`FLAG_JSON`] is the `json` alias. A member list is one
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
//! A field record is [`FIELD_RECORD`] bytes: the same three names, then
//! the offset of the field type's description, the byte offset of the
//! field in an instance, and flags ([`MEMBER_SKIP`], [`FIELD_OMIT_NULL`],
//! [`FIELD_MAY_BE_ABSENT`]).

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

/// An `int`, or the position of an enum's variant: the slot's word.
pub const REPR_INT: u32 = 0;
/// A `float`: the slot's bits.
pub const REPR_FLOAT: u32 = 1;
/// A `bool`: zero or not.
pub const REPR_BOOL: u32 = 2;
/// A reference to an object of the type, zero for `null`.
pub const REPR_REF: u32 = 3;
/// A reference to an object whose header says what it is, with an `int`,
/// `float`, `bool` or enum variant in a box; zero for `null`.
pub const REPR_BOXED: u32 = 4;
/// Two slots: the bits of an `int`, `float` or `bool`, then whether there
/// is one.
pub const REPR_PAIR: u32 = 5;
/// `null`: the slot says nothing.
pub const REPR_NULL: u32 = 6;

/// The description is of the `json` alias.
pub const FLAG_JSON: u32 = 1;

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
